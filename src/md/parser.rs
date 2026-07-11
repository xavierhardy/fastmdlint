//! A line-oriented Markdown block parser that produces a token tree closely
//! matching markdownlint's micromark token tree for the constructs the rules
//! consume: ATX/setext headings, fenced/indented code, thematic breaks,
//! blockquotes, ordered/unordered lists, HTML blocks, paragraphs, and inline
//! code spans/links/emphasis within text.
//!
//! Positions are 1-based. `end_column` is one past the last character
//! (matching micromark). Between blocks, `lineEnding` and `lineEndingBlank`
//! tokens are emitted exactly as micromark does, because whitespace/blank-line
//! rules depend on them.

use super::tokens::{Token, Tree};

/// A logical input line (without its trailing newline) plus whether the
/// source used a newline after it.
struct Line {
    /// Characters of the line, excluding the line terminator.
    chars: Vec<char>,
    /// True if a `\n` (or end) followed; used to emit line endings.
    has_newline: bool,
}

pub struct Parser {
    lines: Vec<Line>,
    tree: Tree,
    in_html_flow: bool,
    /// Normalized labels of reference definitions in the document, used to
    /// decide whether `[text]` is a (shortcut) reference link or literal text.
    defined_labels: std::collections::HashSet<String>,
}

/// Result of scanning a line's leading indentation.
fn count_indent(chars: &[char]) -> usize {
    let mut n = 0;
    for &c in chars {
        if c == ' ' {
            n += 1;
        } else if c == '\t' {
            n += 4 - (n % 4);
        } else {
            break;
        }
    }
    n
}

fn leading_spaces(chars: &[char]) -> usize {
    chars
        .iter()
        .take_while(|c| **c == ' ' || **c == '\t')
        .count()
}

fn is_blank(chars: &[char]) -> bool {
    chars.iter().all(|c| *c == ' ' || *c == '\t')
}

impl Parser {
    pub fn parse(content: &str) -> Tree {
        let lines = split_lines(content);
        let defined_labels = collect_definition_labels(&lines);
        let mut p = Parser {
            lines,
            tree: Tree::default(),
            in_html_flow: false,
            defined_labels,
        };
        p.run();
        p.tree
    }

    fn run(&mut self) {
        let n = self.lines.len();
        let mut i = 0;
        while i < n {
            let chars = &self.lines[i].chars;
            if is_blank(chars) {
                self.emit_blank(i);
                i += 1;
                continue;
            }
            let indent = count_indent(chars);
            // Indented code block (4+ spaces) — only when not a lazy paragraph
            // continuation (handled by treating it as a standalone block here).
            if indent >= 4 {
                i = self.parse_indented_code(i, None);
                continue;
            }
            let trimmed: String = chars.iter().skip(leading_spaces(chars)).collect();
            if let Some(len) = fenced_fence(&trimmed) {
                i = self.parse_fenced_code(i, None, len.0, len.1);
                continue;
            }
            if is_thematic_break(&trimmed) {
                self.parse_thematic_break(i, None);
                i += 1;
                continue;
            }
            if atx_level(&trimmed).is_some() {
                self.parse_atx_heading(i, None);
                i += 1;
                continue;
            }
            if trimmed.starts_with('>') {
                i = self.parse_block_quote(i);
                continue;
            }
            if list_marker(&trimmed).is_some() {
                i = self.parse_list(i, None, 0);
                continue;
            }
            if is_html_block_start(&trimmed) {
                i = self.parse_html_block(i, None);
                continue;
            }
            if is_definition_line(&trimmed) {
                self.parse_definition(i);
                i += 1;
                continue;
            }
            // GFM table: header row followed by a delimiter row with the same
            // number of columns.
            if i + 1 < n
                && chars.contains(&'|')
                && let Some(dcount) = delimiter_row_cells(&self.lines[i + 1].chars)
                && count_cells(chars) == dcount
                && dcount > 0
            {
                i = self.parse_table(i);
                continue;
            }
            // Paragraph (may become a setext heading).
            i = self.parse_paragraph(i, None);
        }
    }

    // --- token construction helpers ---

    fn push(&mut self, mut tok: Token, parent: Option<usize>) -> usize {
        tok.parent = parent;
        let idx = self.tree.tokens.len();
        self.tree.tokens.push(tok);
        if let Some(p) = parent {
            self.tree.tokens[p].children.push(idx);
        } else {
            self.tree.roots.push(idx);
        }
        idx
    }

    fn tok(
        &self,
        kind: &'static str,
        sl: usize,
        sc: usize,
        el: usize,
        ec: usize,
        text: String,
    ) -> Token {
        Token {
            kind,
            start_line: sl,
            start_column: sc,
            end_line: el,
            end_column: ec,
            text,
            children: Vec::new(),
            parent: None,
            in_html_flow: self.in_html_flow,
        }
    }

    /// Emit a `lineEnding` for the end of line `i` (0-based) at column `col`
    /// (1-based, position of the newline = line length+1).
    fn emit_line_ending(&mut self, i: usize, col: usize, parent: Option<usize>) {
        if self.lines[i].has_newline {
            let t = self.tok("lineEnding", i + 1, col, i + 2, 1, "\n".to_string());
            self.push(t, parent);
        }
    }

    fn emit_blank(&mut self, i: usize) {
        // A fully blank line at top level -> lineEndingBlank spanning its
        // newline. micromark represents leading whitespace + blank newline.
        let len = self.lines[i].chars.len();
        if len > 0 {
            // whitespace before blank line end is folded into lineEndingBlank
            // start column 1.
        }
        if self.lines[i].has_newline {
            let t = self.tok(
                "lineEndingBlank",
                i + 1,
                len + 1,
                i + 2,
                1,
                "\n".to_string(),
            );
            self.push(t, None);
        }
    }

    fn line_text(&self, i: usize) -> String {
        self.lines[i].chars.iter().collect()
    }

    fn line_len(&self, i: usize) -> usize {
        self.lines[i].chars.len()
    }

    // --- ATX heading ---
    fn parse_atx_heading(&mut self, i: usize, parent: Option<usize>) {
        let chars = self.lines[i].chars.clone();
        let lead = leading_spaces(&chars);
        let line_no = i + 1;
        let full: String = chars.iter().collect();
        let content_end = chars.len();
        let heading_start_col = lead + 1;
        let heading = self.tok(
            "atxHeading",
            line_no,
            heading_start_col,
            line_no,
            content_end + 1,
            full[byte_off(&chars, lead)..].to_string(),
        );
        let hidx = self.push(heading, parent);

        // Opening sequence of #'s
        let mut pos = lead;
        let seq_start = pos;
        while pos < chars.len() && chars[pos] == '#' {
            pos += 1;
        }
        let open_seq = self.tok(
            "atxHeadingSequence",
            line_no,
            seq_start + 1,
            line_no,
            pos + 1,
            chars[seq_start..pos].iter().collect(),
        );
        self.push(open_seq, Some(hidx));

        // Whitespace after opening sequence
        let ws_start = pos;
        while pos < chars.len() && (chars[pos] == ' ' || chars[pos] == '\t') {
            pos += 1;
        }
        if pos > ws_start {
            let ws = self.tok(
                "whitespace",
                line_no,
                ws_start + 1,
                line_no,
                pos + 1,
                chars[ws_start..pos].iter().collect(),
            );
            self.push(ws, Some(hidx));
        }

        // Trailing: detect closing sequence (spaces + #'s + optional spaces at EOL)
        let mut end = chars.len();
        // trailing whitespace
        let mut te = end;
        while te > pos && (chars[te - 1] == ' ' || chars[te - 1] == '\t') {
            te -= 1;
        }
        let mut hs = te;
        while hs > pos && chars[hs - 1] == '#' {
            hs -= 1;
        }
        let has_closing = hs < te && (hs == pos || chars[hs - 1] == ' ' || chars[hs - 1] == '\t');
        let mut text_end = end;
        let mut close_seq_range: Option<(usize, usize)> = None;
        if has_closing {
            // whitespace between text and closing sequence
            let mut ws2 = hs;
            while ws2 > pos && (chars[ws2 - 1] == ' ' || chars[ws2 - 1] == '\t') {
                ws2 -= 1;
            }
            text_end = ws2;
            close_seq_range = Some((hs, te));
            end = te; // trailing whitespace after close ignored for text
        } else {
            // strip trailing whitespace from text
            while text_end > pos && (chars[text_end - 1] == ' ' || chars[text_end - 1] == '\t') {
                text_end -= 1;
            }
        }

        if text_end > pos {
            let text_tok = self.tok(
                "atxHeadingText",
                line_no,
                pos + 1,
                line_no,
                text_end + 1,
                chars[pos..text_end].iter().collect(),
            );
            let tidx = self.push(text_tok, Some(hidx));
            self.parse_inline(&chars[pos..text_end], line_no, pos + 1, tidx);
        }

        if let Some((cs, ce)) = close_seq_range {
            // whitespace before closing sequence
            if cs > text_end {
                let ws = self.tok(
                    "whitespace",
                    line_no,
                    text_end + 1,
                    line_no,
                    cs + 1,
                    chars[text_end..cs].iter().collect(),
                );
                self.push(ws, Some(hidx));
            }
            let cseq = self.tok(
                "atxHeadingSequence",
                line_no,
                cs + 1,
                line_no,
                ce + 1,
                chars[cs..ce].iter().collect(),
            );
            self.push(cseq, Some(hidx));
        }
        let _ = end;
        self.emit_line_ending(i, self.line_len(i) + 1, parent);
    }

    // --- thematic break ---
    fn parse_thematic_break(&mut self, i: usize, parent: Option<usize>) {
        let chars = self.lines[i].chars.clone();
        let lead = leading_spaces(&chars);
        let line_no = i + 1;
        let tb = self.tok(
            "thematicBreak",
            line_no,
            lead + 1,
            line_no,
            chars.len() + 1,
            chars[lead..].iter().collect(),
        );
        let tbidx = self.push(tb, parent);
        let seq = self.tok(
            "thematicBreakSequence",
            line_no,
            lead + 1,
            line_no,
            chars.len() + 1,
            chars[lead..].iter().collect(),
        );
        self.push(seq, Some(tbidx));
        self.emit_line_ending(i, self.line_len(i) + 1, parent);
    }

    // --- fenced code ---
    fn parse_fenced_code(
        &mut self,
        start: usize,
        parent: Option<usize>,
        fence_char: char,
        fence_len: usize,
    ) -> usize {
        let chars = self.lines[start].chars.clone();
        let lead = leading_spaces(&chars);
        let sl = start + 1;
        // find closing fence

        let mut closed_at: Option<usize> = None;
        let mut j = start + 1;
        while j < self.lines.len() {
            let lc = &self.lines[j].chars;
            let jl = leading_spaces(lc);
            let jt: String = lc.iter().skip(jl).collect();
            if jl < 4
                && let Some((c, l)) = fenced_fence(&jt)
                && c == fence_char
                && l >= fence_len
                && only_fence(&jt, fence_char)
            {
                closed_at = Some(j);
                break;
            }
            j += 1;
        }
        let end_line = closed_at.unwrap_or(self.lines.len() - 1);
        let last_len = self.line_len(end_line);
        let cf = self.tok(
            "codeFenced",
            sl,
            lead + 1,
            end_line + 1,
            last_len + 1,
            String::new(),
        );
        let cfidx = self.push(cf, parent);
        // opening fence
        self.emit_fence_line(start, cfidx, fence_char, true);
        self.emit_line_ending(start, self.line_len(start) + 1, Some(cfidx));
        // content lines
        let content_end = closed_at.unwrap_or(self.lines.len());
        for k in (start + 1)..content_end {
            let klen = self.line_len(k);
            if klen > 0 {
                let cv = self.tok(
                    "codeFlowValue",
                    k + 1,
                    1,
                    k + 1,
                    klen + 1,
                    self.line_text(k),
                );
                // Note: leading indent up to fence indent stripped in micromark;
                // approximated here as full-line codeFlowValue.
                self.push(cv, Some(cfidx));
            }
            self.emit_line_ending(k, klen + 1, Some(cfidx));
        }
        if let Some(cl) = closed_at {
            self.emit_fence_line(cl, cfidx, fence_char, false);
            self.emit_line_ending(cl, self.line_len(cl) + 1, parent);
            self.fix_text(cfidx);
            return cl + 1;
        }
        self.fix_text(cfidx);
        self.lines.len()
    }

    fn emit_fence_line(&mut self, i: usize, cfidx: usize, fence_char: char, opening: bool) {
        let chars = self.lines[i].chars.clone();
        let lead = leading_spaces(&chars);
        let line_no = i + 1;
        let mut pos = lead;
        while pos < chars.len() && chars[pos] == fence_char {
            pos += 1;
        }
        let fence_end = chars.len();
        let fence = self.tok(
            "codeFencedFence",
            line_no,
            lead + 1,
            line_no,
            fence_end + 1,
            chars[lead..].iter().collect(),
        );
        let fidx = self.push(fence, Some(cfidx));
        let seq = self.tok(
            "codeFencedFenceSequence",
            line_no,
            lead + 1,
            line_no,
            pos + 1,
            chars[lead..pos].iter().collect(),
        );
        self.push(seq, Some(fidx));
        if opening {
            // info string
            let mut ws = pos;
            while ws < chars.len() && (chars[ws] == ' ' || chars[ws] == '\t') {
                ws += 1;
            }
            if ws < chars.len() {
                let info = self.tok(
                    "codeFencedFenceInfo",
                    line_no,
                    ws + 1,
                    line_no,
                    chars.len() + 1,
                    chars[ws..].iter().collect(),
                );
                let iidx = self.push(info, Some(fidx));
                let data = self.tok(
                    "data",
                    line_no,
                    ws + 1,
                    line_no,
                    chars.len() + 1,
                    chars[ws..].iter().collect(),
                );
                self.push(data, Some(iidx));
            }
        }
    }

    // --- indented code ---
    fn parse_indented_code(&mut self, start: usize, parent: Option<usize>) -> usize {
        let sl = start + 1;
        let mut j = start;
        // Consume consecutive indented (or blank between) lines.
        let mut last = start;
        while j < self.lines.len() {
            let lc = &self.lines[j].chars;
            if is_blank(lc) {
                // blank line: part of code block only if followed by more code
                let mut k = j + 1;
                let mut more = false;
                while k < self.lines.len() {
                    if is_blank(&self.lines[k].chars) {
                        k += 1;
                        continue;
                    }
                    if count_indent(&self.lines[k].chars) >= 4 {
                        more = true;
                    }
                    break;
                }
                if more {
                    j = k;
                    continue;
                } else {
                    break;
                }
            }
            if count_indent(lc) >= 4 {
                last = j;
                j += 1;
            } else {
                break;
            }
        }
        let last_len = self.line_len(last);
        let ci = self.tok("codeIndented", sl, 1, last + 1, last_len + 1, String::new());
        let ciidx = self.push(ci, parent);
        for k in start..=last {
            let lc = self.lines[k].chars.clone();
            if is_blank(&lc) {
                self.emit_line_ending(k, self.line_len(k) + 1, Some(ciidx));
                continue;
            }
            let line_no = k + 1;
            // linePrefix: first 4 columns
            let mut cut = 0;
            let mut col = 0;
            while cut < lc.len() && col < 4 {
                if lc[cut] == '\t' {
                    col += 4 - (col % 4);
                } else {
                    col += 1;
                }
                cut += 1;
            }
            let lp = self.tok(
                "linePrefix",
                line_no,
                1,
                line_no,
                cut + 1,
                lc[..cut].iter().collect(),
            );
            self.push(lp, Some(ciidx));
            if cut < lc.len() {
                let cv = self.tok(
                    "codeFlowValue",
                    line_no,
                    cut + 1,
                    line_no,
                    lc.len() + 1,
                    lc[cut..].iter().collect(),
                );
                self.push(cv, Some(ciidx));
            }
            self.emit_line_ending(k, self.line_len(k) + 1, Some(ciidx));
        }
        self.fix_text(ciidx);
        last + 1
    }

    // --- GFM table ---
    fn parse_table(&mut self, start: usize) -> usize {
        // Body rows continue until a blank line or a line that starts a
        // different block.
        let mut end = start + 1; // header + delimiter
        let mut k = start + 2;
        while k < self.lines.len() {
            let lc = &self.lines[k].chars;
            if is_blank(lc) {
                break;
            }
            let lead = leading_spaces(lc);
            let t: String = lc.iter().skip(lead).collect();
            if lead < 4
                && (atx_level(&t).is_some()
                    || fenced_fence(&t).is_some()
                    || is_thematic_break(&t)
                    || t.starts_with('>')
                    || is_html_block_start(&t))
            {
                break;
            }
            end = k;
            k += 1;
        }
        let last_len = self.line_len(end);
        let table = self.tok("table", start + 1, 1, end + 1, last_len + 1, String::new());
        let tidx = self.push(table, None);

        // Head: header row + delimiter row.
        let head_last_len = self.line_len(start + 1);
        let head = self.tok(
            "tableHead",
            start + 1,
            1,
            start + 2,
            head_last_len + 1,
            String::new(),
        );
        let hidx = self.push(head, Some(tidx));
        self.emit_table_row(start, hidx, "tableHeader");
        self.emit_line_ending(start, self.line_len(start) + 1, Some(hidx));
        self.emit_table_row(start + 1, hidx, "tableDelimiter");

        self.emit_line_ending(start + 1, self.line_len(start + 1) + 1, Some(tidx));

        if end >= start + 2 {
            let body = self.tok(
                "tableBody",
                start + 3,
                1,
                end + 1,
                last_len + 1,
                String::new(),
            );
            let bidx = self.push(body, Some(tidx));
            for row in (start + 2)..=end {
                self.emit_table_row(row, bidx, "tableData");
                if row < end {
                    self.emit_line_ending(row, self.line_len(row) + 1, Some(bidx));
                }
            }
        }
        self.emit_line_ending(end, last_len + 1, None);
        end + 1
    }

    fn emit_table_row(&mut self, k: usize, parent: usize, cell_kind: &'static str) {
        let lc = self.lines[k].chars.clone();
        let line_no = k + 1;
        let lead = leading_spaces(&lc);
        let cells = compute_cells(&lc, lead);
        let is_delim = cell_kind == "tableDelimiter";
        let row_kind = if is_delim {
            "tableDelimiterRow"
        } else {
            "tableRow"
        };
        let row = self.tok(
            row_kind,
            line_no,
            lead + 1,
            line_no,
            lc.len() + 1,
            lc.iter().collect(),
        );
        let ridx = self.push(row, Some(parent));
        for (cs, ce) in cells {
            let cell = self.tok(
                cell_kind,
                line_no,
                cs + 1,
                line_no,
                ce + 1,
                lc[cs..ce].iter().collect(),
            );
            let cidx = self.push(cell, Some(ridx));
            let mut p = cs;
            if p < ce && lc[p] == '|' {
                let d = self.tok(
                    "tableCellDivider",
                    line_no,
                    p + 1,
                    line_no,
                    p + 2,
                    "|".into(),
                );
                self.push(d, Some(cidx));
                p += 1;
            }
            // leading whitespace
            let mut q = p;
            while q < ce && (lc[q] == ' ' || lc[q] == '\t') {
                q += 1;
            }
            if q > p {
                let w = self.tok(
                    "whitespace",
                    line_no,
                    p + 1,
                    line_no,
                    q + 1,
                    lc[p..q].iter().collect(),
                );
                self.push(w, Some(cidx));
            }
            p = q;
            // detect trailing divider
            let mut t = ce;
            while t > p && (lc[t - 1] == ' ' || lc[t - 1] == '\t') {
                t -= 1;
            }
            let (content_end, trailing_div) = if t > p && lc[t - 1] == '|' {
                (t - 1, Some(t - 1))
            } else {
                (ce, None)
            };
            // trim trailing ws from content
            let mut c_end = content_end;
            while c_end > p && (lc[c_end - 1] == ' ' || lc[c_end - 1] == '\t') {
                c_end -= 1;
            }
            if c_end > p {
                let content = self.tok(
                    "tableContent",
                    line_no,
                    p + 1,
                    line_no,
                    c_end + 1,
                    lc[p..c_end].iter().collect(),
                );
                let coidx = self.push(content, Some(cidx));
                let child_kind = if is_delim {
                    "tableDelimiterFiller"
                } else {
                    "data"
                };
                let child = self.tok(
                    child_kind,
                    line_no,
                    p + 1,
                    line_no,
                    c_end + 1,
                    lc[p..c_end].iter().collect(),
                );
                self.push(child, Some(coidx));
            }
            // trailing whitespace before divider (or to cell end)
            let ws_end = content_end;
            if ws_end > c_end {
                let w = self.tok(
                    "whitespace",
                    line_no,
                    c_end + 1,
                    line_no,
                    ws_end + 1,
                    lc[c_end..ws_end].iter().collect(),
                );
                self.push(w, Some(cidx));
            }
            if let Some(dp) = trailing_div {
                let d = self.tok(
                    "tableCellDivider",
                    line_no,
                    dp + 1,
                    line_no,
                    dp + 2,
                    "|".into(),
                );
                self.push(d, Some(cidx));
                if ce > dp + 1 {
                    let w = self.tok(
                        "whitespace",
                        line_no,
                        dp + 2,
                        line_no,
                        ce + 1,
                        lc[dp + 1..ce].iter().collect(),
                    );
                    self.push(w, Some(cidx));
                }
            }
        }
    }

    // --- link reference definition (opaque) ---
    fn parse_definition(&mut self, i: usize) {
        let chars = self.lines[i].chars.clone();
        let line_no = i + 1;
        let def = self.tok(
            "definition",
            line_no,
            1,
            line_no,
            chars.len() + 1,
            chars.iter().collect(),
        );
        // Emitted opaque: no inline children, so URLs inside are not scanned.
        self.push(def, None);
        self.emit_line_ending(i, chars.len() + 1, None);
    }

    // --- HTML block (coarse) ---
    fn parse_html_block(&mut self, start: usize, parent: Option<usize>) -> usize {
        // Consume until a blank line.
        let mut j = start;
        while j < self.lines.len() && !is_blank(&self.lines[j].chars) {
            j += 1;
        }
        let end = j - 1;
        let sl = start + 1;
        let last_len = self.line_len(end);
        let mut text = String::new();
        for k in start..=end {
            if k > start {
                text.push('\n');
            }
            text.push_str(&self.line_text(k));
        }
        let hf = self.tok("htmlFlow", sl, 1, end + 1, last_len + 1, text.clone());
        let hidx = self.push(hf, parent);
        // Re-parse the block's inline content (as micromark does) so that HTML
        // tags become `htmlText` children marked as being in an htmlFlow.
        // Skipped for HTML comments (which rules treat opaquely).
        if !text.trim_start().starts_with("<!--") {
            let was = self.in_html_flow;
            self.in_html_flow = true;
            let content = self.tok("content", sl, 1, end + 1, last_len + 1, text.clone());
            let cidx = self.push(content, Some(hidx));
            let para = self.tok("paragraph", sl, 1, end + 1, last_len + 1, text);
            let pidx = self.push(para, Some(cidx));
            for k in start..=end {
                let lc = self.lines[k].chars.clone();
                self.parse_inline(&lc, k + 1, 1, pidx);
                if k < end {
                    let t = self.tok(
                        "lineEnding",
                        k + 1,
                        self.line_len(k) + 1,
                        k + 2,
                        1,
                        "\n".to_string(),
                    );
                    self.push(t, Some(pidx));
                }
            }
            self.in_html_flow = was;
        }
        for k in start..=end {
            self.emit_line_ending(k, self.line_len(k) + 1, parent);
        }
        end + 1
    }

    // --- block quote (coarse: recursively parse inner content stripped of prefix) ---
    fn parse_block_quote(&mut self, start: usize) -> usize {
        // Collect consecutive block-quote lines.
        let mut j = start;
        while j < self.lines.len() {
            let lc = &self.lines[j].chars;
            let lead = leading_spaces(lc);
            if lead < 4 && lc.get(lead) == Some(&'>') {
                j += 1;
            } else if !is_blank(lc) && j > start {
                // lazy continuation of paragraph
                let lead2 = leading_spaces(lc);
                let t: String = lc.iter().skip(lead2).collect();
                if list_marker(&t).is_none()
                    && atx_level(&t).is_none()
                    && fenced_fence(&t).is_none()
                    && !is_thematic_break(&t)
                {
                    j += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        let end = j - 1;
        let sl = start + 1;
        let last_len = self.line_len(end);
        let bq = self.tok("blockQuote", sl, 1, end + 1, last_len + 1, String::new());
        let bqidx = self.push(bq, None);

        // If the blockquote's content is a list, parse it as a container so
        // that list rules (MD007 etc.) see the nested structure with correct
        // columns. Requires a uniform `>`/`> ` prefix on every line.
        if let Some(wmap) = uniform_bq_prefixes(&self.lines, start, end) {
            let first: String = {
                let lc = &self.lines[start].chars;
                lc.iter().skip(wmap[0]).collect()
            };
            if list_marker(&first).is_some() {
                self.emit_bq_list(start, end, bqidx, &wmap);
                self.emit_line_ending(end, last_len + 1, None);
                return end + 1;
            }
        }

        // Line 1 prefix is a direct child of the blockQuote.
        let first_content_col = self.emit_bq_prefix(start, bqidx);
        let content = self.tok(
            "content",
            sl,
            first_content_col + 1,
            end + 1,
            last_len + 1,
            String::new(),
        );
        let cidx = self.push(content, Some(bqidx));
        let para = self.tok(
            "paragraph",
            sl,
            first_content_col + 1,
            end + 1,
            last_len + 1,
            String::new(),
        );
        let pidx = self.push(para, Some(cidx));

        for k in start..=end {
            let content_col = if k == start {
                first_content_col
            } else {
                // lineEnding then this line's prefix, inside the paragraph.
                let t = self.tok(
                    "lineEnding",
                    k,
                    self.line_len(k - 1) + 1,
                    k + 1,
                    1,
                    "\n".to_string(),
                );
                self.push(t, Some(pidx));
                self.emit_bq_prefix(k, pidx)
            };
            let lc = self.lines[k].chars.clone();
            if content_col < lc.len() {
                self.parse_inline(&lc[content_col..], k + 1, content_col + 1, pidx);
            }
        }
        self.emit_line_ending(end, last_len + 1, None);
        end + 1
    }

    /// Emit the blockquote prefix (and any extra-space `linePrefix`) for line
    /// `k` under `parent`; returns the 0-based content column.
    fn emit_bq_prefix(&mut self, k: usize, parent: usize) -> usize {
        let lc = self.lines[k].chars.clone();
        let line_no = k + 1;
        let lead = leading_spaces(&lc);
        if lead >= 4 || lc.get(lead) != Some(&'>') {
            return leading_spaces(&lc).min(lc.len()); // lazy continuation
        }
        if lead > 0 {
            let lp = self.tok(
                "linePrefix",
                line_no,
                1,
                line_no,
                lead + 1,
                lc[..lead].iter().collect(),
            );
            self.push(lp, Some(parent));
        }
        let marker_col = lead;
        let has_space = lc.get(marker_col + 1) == Some(&' ');
        let pe = if has_space {
            marker_col + 2
        } else {
            marker_col + 1
        };
        let prefix = self.tok(
            "blockQuotePrefix",
            line_no,
            marker_col + 1,
            line_no,
            pe + 1,
            lc[marker_col..pe].iter().collect(),
        );
        let pidx = self.push(prefix, Some(parent));
        let marker = self.tok(
            "blockQuoteMarker",
            line_no,
            marker_col + 1,
            line_no,
            marker_col + 2,
            ">".into(),
        );
        self.push(marker, Some(pidx));
        if has_space {
            let w = self.tok(
                "blockQuotePrefixWhitespace",
                line_no,
                marker_col + 2,
                line_no,
                pe + 1,
                " ".into(),
            );
            self.push(w, Some(pidx));
        }
        let mut ee = pe;
        while ee < lc.len() && lc[ee] == ' ' {
            ee += 1;
        }
        if ee > pe {
            let lp = self.tok(
                "linePrefix",
                line_no,
                pe + 1,
                line_no,
                ee + 1,
                lc[pe..ee].iter().collect(),
            );
            self.push(lp, Some(parent));
        }
        ee
    }

    /// Emit just the `blockQuotePrefix` (marker + optional single space) for
    /// line `k` under `parent`; no extra-space `linePrefix`.
    fn emit_bq_prefix_marker(&mut self, k: usize, parent: usize) {
        let lc = self.lines[k].chars.clone();
        let line_no = k + 1;
        let lead = leading_spaces(&lc);
        if lead >= 4 || lc.get(lead) != Some(&'>') {
            return;
        }
        let marker_col = lead;
        let has_space = lc.get(marker_col + 1) == Some(&' ');
        let pe = if has_space {
            marker_col + 2
        } else {
            marker_col + 1
        };
        let prefix = self.tok(
            "blockQuotePrefix",
            line_no,
            marker_col + 1,
            line_no,
            pe + 1,
            lc[marker_col..pe].iter().collect(),
        );
        let pidx = self.push(prefix, Some(parent));
        let marker = self.tok(
            "blockQuoteMarker",
            line_no,
            marker_col + 1,
            line_no,
            marker_col + 2,
            ">".into(),
        );
        self.push(marker, Some(pidx));
        if has_space {
            let w = self.tok(
                "blockQuotePrefixWhitespace",
                line_no,
                marker_col + 2,
                line_no,
                pe + 1,
                " ".into(),
            );
            self.push(w, Some(pidx));
        }
    }

    /// Parse a blockquote whose content is a list: sub-parse the de-prefixed
    /// content and re-emit it under the blockquote with column offsets and
    /// interleaved `blockQuotePrefix` tokens.
    fn emit_bq_list(&mut self, start: usize, end: usize, bqidx: usize, wmap: &[usize]) {
        let mut stripped = String::new();
        for (i, k) in (start..=end).enumerate() {
            if i > 0 {
                stripped.push('\n');
            }
            let lc = &self.lines[k].chars;
            stripped.extend(lc.iter().skip(wmap[i]));
        }
        let sub = Parser::parse(&stripped);
        self.emit_bq_prefix_marker(start, bqidx);
        let roots = sub.roots.clone();
        for r in roots {
            self.reemit_bq(&sub, r, bqidx, wmap, start, end);
        }
    }

    fn reemit_bq(
        &mut self,
        sub: &Tree,
        sidx: usize,
        new_parent: usize,
        wmap: &[usize],
        start: usize,
        end: usize,
    ) {
        let s = &sub.tokens[sidx];
        let (kind, ssl, ssc, sel, sec) = (
            s.kind,
            s.start_line,
            s.start_column,
            s.end_line,
            s.end_column,
        );
        let text = s.text.clone();
        let children = s.children.clone();
        let wstart = wmap.get(ssl.saturating_sub(1)).copied().unwrap_or(0);
        let wend = wmap.get(sel.saturating_sub(1)).copied().unwrap_or(0);
        let tok = self.tok(
            kind,
            start + ssl,
            ssc + wstart,
            start + sel,
            sec + wend,
            text,
        );
        let nidx = self.push(tok, Some(new_parent));
        for c in children {
            self.reemit_bq(sub, c, nidx, wmap, start, end);
        }
        if kind == "lineEnding" {
            let next0 = start + ssl; // 0-based original line after the ending
            if next0 <= end {
                self.emit_bq_prefix_marker(next0, new_parent);
            }
        }
    }

    // --- list (CommonMark-ish, single pass) ---
    //
    // `min_indent` is the lowest column (0-based) at which a marker still
    // belongs to this list — for a top-level list it is 0; for a nested list
    // it is the content column of the parent item. A marker at indent >= the
    // current item's content column begins a nested list.
    fn parse_list(&mut self, start: usize, parent: Option<usize>, min_indent: usize) -> usize {
        let first_chars = self.lines[start].chars.clone();
        let lead = leading_spaces(&first_chars);
        let ft: String = first_chars.iter().skip(lead).collect();
        let (ordered, first_marker, _) = list_marker(&ft).unwrap();
        let kind = if ordered {
            "listOrdered"
        } else {
            "listUnordered"
        };
        let first_sig = (ordered, first_marker);

        let list = self.tok(kind, start + 1, lead + 1, start + 1, 1, String::new());
        let lidx = self.push(list, parent);

        let mut cur_content_col = self.content_col(start);
        let mut last_line = start;
        let mut k = start;
        let mut first = true;
        while k < self.lines.len() {
            let lc = self.lines[k].chars.clone();
            if is_blank(&lc) {
                let mut kk = k + 1;
                while kk < self.lines.len() && is_blank(&self.lines[kk].chars) {
                    kk += 1;
                }
                let cont = if kk < self.lines.len() {
                    let ki = leading_spaces(&self.lines[kk].chars);
                    let kt: String = self.lines[kk].chars.iter().skip(ki).collect();
                    let same_sig = list_marker(&kt)
                        .map(|(o, m, _)| (o, m) == first_sig)
                        .unwrap_or(false);
                    ki >= cur_content_col || (ki >= min_indent && same_sig)
                } else {
                    false
                };
                if cont {
                    self.emit_line_ending(k, self.line_len(k) + 1, Some(lidx));
                    k += 1;
                    continue;
                }
                break;
            }
            let i = leading_spaces(&lc);
            let t: String = lc.iter().skip(i).collect();
            let line_no = k + 1;
            if first {
                cur_content_col = self.emit_item(k, lidx);
                last_line = k;
                self.emit_line_ending(k, self.line_len(k) + 1, Some(lidx));
                first = false;
                k += 1;
                continue;
            }
            // Deeper-indented marker -> nested list under the current item.
            if i >= cur_content_col && list_marker(&t).is_some() {
                let next = self.parse_list(k, Some(lidx), cur_content_col);
                last_line = next - 1;
                k = next;
                continue;
            }
            if i < min_indent {
                break;
            }
            if i < cur_content_col {
                match list_marker(&t) {
                    Some((o, m, _)) if (o, m) == first_sig => {
                        cur_content_col = self.emit_item(k, lidx);
                        last_line = k;
                        self.emit_line_ending(k, self.line_len(k) + 1, Some(lidx));
                        k += 1;
                        continue;
                    }
                    Some(_) => break, // marker change -> new list
                    None => {
                        // A dedented line that starts a new block ends the list;
                        // only plain text lazily continues an item's paragraph.
                        if fenced_fence(&t).is_some()
                            || atx_level(&t).is_some()
                            || is_thematic_break(&t)
                            || t.starts_with('>')
                            || is_html_block_start(&t)
                        {
                            break;
                        }
                        if i > 0 {
                            let ind = self.tok(
                                "listItemIndent",
                                line_no,
                                1,
                                line_no,
                                i + 1,
                                lc[..i].iter().collect(),
                            );
                            self.push(ind, Some(lidx));
                        }
                        last_line = k;
                        self.emit_line_ending(k, self.line_len(k) + 1, Some(lidx));
                        k += 1;
                        continue;
                    }
                }
            }
            // i >= cur_content_col, non-marker: item continuation content.
            let ind = self.tok(
                "listItemIndent",
                line_no,
                1,
                line_no,
                i + 1,
                lc[..i].iter().collect(),
            );
            self.push(ind, Some(lidx));
            last_line = k;
            self.emit_line_ending(k, self.line_len(k) + 1, Some(lidx));
            k += 1;
        }

        let last_len = self.line_len(last_line);
        self.tree.tokens[lidx].end_line = last_line + 1;
        self.tree.tokens[lidx].end_column = last_len + 1;
        last_line + 1
    }

    /// Content column (0-based index of the first content char) of a list-item
    /// marker line.
    fn content_col(&self, k: usize) -> usize {
        let lc = &self.lines[k].chars;
        let klead = leading_spaces(lc);
        let kt: String = lc.iter().skip(klead).collect();
        let (_o, _m, mlen) = match list_marker(&kt) {
            Some(v) => v,
            None => return klead,
        };
        let mut pos = klead + mlen;
        while pos < lc.len() && (lc[pos] == ' ' || lc[pos] == '\t') {
            pos += 1;
        }
        pos.min(lc.len())
    }

    /// Emit a `listItemPrefix` (+ marker/value/whitespace) and the item's
    /// first-line `content`/`paragraph`. Returns the content column.
    fn emit_item(&mut self, k: usize, lidx: usize) -> usize {
        let lc = self.lines[k].chars.clone();
        let klead = leading_spaces(&lc);
        let kt: String = lc.iter().skip(klead).collect();
        let (ord, _m, mlen) = list_marker(&kt).unwrap();
        let line_no = k + 1;
        let mut after = klead + mlen;
        let mut ws_after = after;
        while ws_after < lc.len() && (lc[ws_after] == ' ' || lc[ws_after] == '\t') {
            ws_after += 1;
        }
        let prefix_end = ws_after.min(lc.len());
        let prefix = self.tok(
            "listItemPrefix",
            line_no,
            klead + 1,
            line_no,
            prefix_end + 1,
            lc[klead..prefix_end].iter().collect(),
        );
        let pidx = self.push(prefix, Some(lidx));
        if ord {
            let mut d = klead;
            while d < lc.len() && lc[d].is_ascii_digit() {
                d += 1;
            }
            let val = self.tok(
                "listItemValue",
                line_no,
                klead + 1,
                line_no,
                d + 1,
                lc[klead..d].iter().collect(),
            );
            self.push(val, Some(pidx));
            let marker = self.tok(
                "listItemMarker",
                line_no,
                d + 1,
                line_no,
                d + 2,
                lc[d..d + 1].iter().collect(),
            );
            self.push(marker, Some(pidx));
            after = d + 1;
        } else {
            let marker = self.tok(
                "listItemMarker",
                line_no,
                klead + 1,
                line_no,
                klead + 2,
                lc[klead..klead + 1].iter().collect(),
            );
            self.push(marker, Some(pidx));
            after = klead + 1;
        }
        if ws_after > after {
            let wsw = self.tok(
                "listItemPrefixWhitespace",
                line_no,
                after + 1,
                line_no,
                ws_after + 1,
                lc[after..ws_after].iter().collect(),
            );
            self.push(wsw, Some(pidx));
        }
        if prefix_end < lc.len() {
            let content = self.tok(
                "content",
                line_no,
                prefix_end + 1,
                line_no,
                lc.len() + 1,
                lc[prefix_end..].iter().collect(),
            );
            let cidx = self.push(content, Some(lidx));
            let para = self.tok(
                "paragraph",
                line_no,
                prefix_end + 1,
                line_no,
                lc.len() + 1,
                lc[prefix_end..].iter().collect(),
            );
            let paidx = self.push(para, Some(cidx));
            self.parse_inline(&lc[prefix_end..], line_no, prefix_end + 1, paidx);
        }
        prefix_end
    }

    // --- paragraph / setext ---
    fn parse_paragraph(&mut self, start: usize, parent: Option<usize>) -> usize {
        // Gather consecutive non-blank lines that don't start a new block.
        let mut j = start;
        let mut setext_line: Option<usize> = None;
        while j < self.lines.len() {
            let lc = &self.lines[j].chars;
            if is_blank(lc) {
                break;
            }
            let lead = leading_spaces(lc);
            let t: String = lc.iter().skip(lead).collect();
            if j > start {
                // setext underline?
                if lead < 4 && is_setext_underline(&t) {
                    setext_line = Some(j);
                    break;
                }
                if lead < 4
                    && (atx_level(&t).is_some()
                        || fenced_fence(&t).is_some()
                        || is_thematic_break(&t)
                        || t.starts_with('>')
                        || is_html_block_start(&t)
                        || interrupts_paragraph_list(&t))
                {
                    break;
                }
                // A table (header + delimiter row) interrupts a paragraph.
                if lc.contains(&'|')
                    && j + 1 < self.lines.len()
                    && let Some(dc) = delimiter_row_cells(&self.lines[j + 1].chars)
                    && count_cells(lc) == dc
                    && dc > 0
                {
                    break;
                }
            }
            j += 1;
        }
        let end = j - 1;
        if let Some(sl_line) = setext_line {
            self.emit_setext(start, end, sl_line, parent);
            return sl_line + 1;
        }
        self.emit_paragraph(start, end, parent);
        end + 1
    }

    fn emit_paragraph(&mut self, start: usize, end: usize, parent: Option<usize>) {
        let sl = start + 1;
        let first_lead = leading_spaces(&self.lines[start].chars);
        let last_len = self.line_len(end);
        let content = self.tok(
            "content",
            sl,
            first_lead + 1,
            end + 1,
            last_len + 1,
            String::new(),
        );
        let cidx = self.push(content, parent);
        let para = self.tok(
            "paragraph",
            sl,
            first_lead + 1,
            end + 1,
            last_len + 1,
            String::new(),
        );
        let pidx = self.push(para, Some(cidx));
        for k in start..=end {
            let lc = self.lines[k].chars.clone();
            let lead = if k == start {
                first_lead
            } else {
                leading_spaces(&lc)
            };
            let line_no = k + 1;
            self.parse_inline(&lc[lead..], line_no, lead + 1, pidx);
            if k < end {
                let t = self.tok(
                    "lineEnding",
                    line_no,
                    lc.len() + 1,
                    line_no + 1,
                    1,
                    "\n".to_string(),
                );
                self.push(t, Some(pidx));
            }
        }
        self.emit_line_ending(end, last_len + 1, parent);
    }

    fn emit_setext(&mut self, start: usize, end: usize, underline: usize, parent: Option<usize>) {
        let sl = start + 1;
        let ul = underline;
        let ul_lead = leading_spaces(&self.lines[ul].chars);
        let ul_len = self.line_len(ul);
        let sh = self.tok("setextHeading", sl, 1, ul + 1, ul_len + 1, String::new());
        let shidx = self.push(sh, parent);
        // setextHeadingText spanning start..=end
        let first_lead = leading_spaces(&self.lines[start].chars);
        let text_last_len = self.line_len(end);
        let text = self.tok(
            "setextHeadingText",
            sl,
            first_lead + 1,
            end + 1,
            text_last_len + 1,
            String::new(),
        );
        let tidx = self.push(text, Some(shidx));
        for k in start..=end {
            let lc = self.lines[k].chars.clone();
            let lead = if k == start {
                first_lead
            } else {
                leading_spaces(&lc)
            };
            let line_no = k + 1;
            self.parse_inline(&lc[lead..], line_no, lead + 1, tidx);
            if k < end {
                let t = self.tok(
                    "lineEnding",
                    line_no,
                    lc.len() + 1,
                    line_no + 1,
                    1,
                    "\n".to_string(),
                );
                self.push(t, Some(tidx));
            }
        }
        // lineEnding between text and underline
        let te = self.tok(
            "lineEnding",
            end + 1,
            text_last_len + 1,
            end + 2,
            1,
            "\n".to_string(),
        );
        self.push(te, Some(shidx));
        // setextHeadingLine
        let uline = self.tok(
            "setextHeadingLine",
            ul + 1,
            ul_lead + 1,
            ul + 1,
            ul_len + 1,
            self.lines[ul].chars[ul_lead..].iter().collect(),
        );
        let ulidx = self.push(uline, Some(shidx));
        let useq = self.tok(
            "setextHeadingLineSequence",
            ul + 1,
            ul_lead + 1,
            ul + 1,
            ul_len + 1,
            self.lines[ul].chars[ul_lead..].iter().collect(),
        );
        self.push(useq, Some(ulidx));
        self.emit_line_ending(ul, ul_len + 1, parent);
    }

    // --- inline: data, code spans, autolinks, raw HTML, links/images,
    //     literal autolinks ---
    /// Character ranges of inline constructs (code, autolinks, HTML,
    /// links/images, literal autolinks), used to mask emphasis delimiters.
    fn construct_ranges(&self, chars: &[char]) -> Vec<(usize, usize)> {
        let mut ranges = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '\\' {
                i += 2;
                continue;
            }
            if c == '`' {
                let mut n = 0;
                while i + n < chars.len() && chars[i + n] == '`' {
                    n += 1;
                }
                let mut j = i + n;
                let mut found = None;
                while j < chars.len() {
                    if chars[j] == '`' {
                        let mut m = 0;
                        while j + m < chars.len() && chars[j + m] == '`' {
                            m += 1;
                        }
                        if m == n {
                            found = Some(j);
                            break;
                        }
                        j += m;
                    } else {
                        j += 1;
                    }
                }
                if let Some(close) = found {
                    let end = close + n;
                    ranges.push((i, end));
                    i = end;
                    continue;
                }
            }
            if c == '<'
                && let Some(end) = detect_autolink(chars, i).or_else(|| detect_html(chars, i))
            {
                ranges.push((i, end));
                i = end;
                continue;
            }
            if (c == '[' || (c == '!' && chars.get(i + 1) == Some(&'[')))
                && let Some(end) = detect_link(chars, i)
            {
                ranges.push((i, end));
                i = end;
                continue;
            }
            if is_url_boundary(chars, i)
                && let Some(end) = detect_literal(chars, i)
            {
                ranges.push((i, end));
                i = end;
                continue;
            }
            i += 1;
        }
        ranges
    }

    fn parse_inline(&mut self, chars: &[char], line_no: usize, start_col: usize, parent: usize) {
        let masked = self.construct_ranges(chars);
        let spans = emphasis_spans(chars, &masked);
        // Opens sorted by (open_start asc, extent desc = outer first).
        let mut opens: Vec<EmphSpan> = spans.clone();
        opens.sort_by(|a, b| {
            a.open_start
                .cmp(&b.open_start)
                .then((b.close_start + b.close_len).cmp(&(a.close_start + a.close_len)))
        });
        let mut op = 0usize;

        struct Frame {
            emph_idx: usize,
            close_start: usize,
            close_len: usize,
            close_kind: &'static str,
        }
        let mut frames: Vec<Frame> = Vec::new();
        let mut cur = parent;

        // Positions of emphasis markers actually consumed by a span; the rest
        // are emitted as standalone `data` tokens (like micromark).
        let mut consumed = std::collections::HashSet::new();
        for s in &spans {
            for p in s.open_start..s.open_start + s.open_len {
                consumed.insert(p);
            }
            for p in s.close_start..s.close_start + s.close_len {
                consumed.insert(p);
            }
        }

        let mut i = 0;
        let mut data_start = 0;
        // `[` position not to re-attempt as a link start (set when a `![`
        // image attempt fails; micromark consumes `![` as one label start).
        let mut suppressed_link_at: Option<usize> = None;
        while i < chars.len() {
            // Emphasis boundaries at i.
            let mut moved = true;
            while moved {
                moved = false;
                if let Some(f) = frames.last()
                    && f.close_start == i
                {
                    self.flush_data(data_start, i, chars, line_no, start_col, cur);
                    let f = frames.pop().unwrap();
                    // closing sequence under the emphasis token
                    let seq = self.tok(
                        f.close_kind,
                        line_no,
                        start_col + i,
                        line_no,
                        start_col + i + f.close_len,
                        chars[i..i + f.close_len].iter().collect(),
                    );
                    self.push(seq, Some(f.emph_idx));
                    i += f.close_len;
                    data_start = i;
                    cur = frames
                        .last()
                        .map(|fr| self.tree.tokens[fr.emph_idx].children[1])
                        .unwrap_or(parent);
                    moved = true;
                    continue;
                }
                while op < opens.len() && opens[op].open_start < i {
                    op += 1;
                }
                if op < opens.len() && opens[op].open_start == i {
                    let span = opens[op];
                    op += 1;
                    self.flush_data(data_start, i, chars, line_no, start_col, cur);
                    let strong = span.open_len == 2;
                    let (kind, seq_kind, text_kind) = if strong {
                        ("strong", "strongSequence", "strongText")
                    } else {
                        ("emphasis", "emphasisSequence", "emphasisText")
                    };
                    let close_end = span.close_start + span.close_len;
                    let emph = self.tok(
                        kind,
                        line_no,
                        start_col + i,
                        line_no,
                        start_col + close_end,
                        chars[i..close_end].iter().collect(),
                    );
                    let eidx = self.push(emph, Some(cur));
                    let oseq = self.tok(
                        seq_kind,
                        line_no,
                        start_col + i,
                        line_no,
                        start_col + i + span.open_len,
                        chars[i..i + span.open_len].iter().collect(),
                    );
                    self.push(oseq, Some(eidx));
                    let text = self.tok(
                        text_kind,
                        line_no,
                        start_col + i + span.open_len,
                        line_no,
                        start_col + span.close_start,
                        chars[i + span.open_len..span.close_start].iter().collect(),
                    );
                    let tidx = self.push(text, Some(eidx));
                    frames.push(Frame {
                        emph_idx: eidx,
                        close_start: span.close_start,
                        close_len: span.close_len,
                        close_kind: seq_kind,
                    });
                    cur = tidx;
                    i += span.open_len;
                    data_start = i;
                    moved = true;
                }
            }
            if i >= chars.len() {
                break;
            }
            let c = chars[i];
            if c == '\\' {
                i += 2;
                continue;
            }
            // An unmatched emphasis marker run is its own `data` token.
            if (c == '*' || c == '_') && !consumed.contains(&i) {
                let mut run_end = i;
                while run_end < chars.len() && chars[run_end] == c && !consumed.contains(&run_end) {
                    run_end += 1;
                }
                self.flush_data(data_start, i, chars, line_no, start_col, cur);
                let d = self.tok(
                    "data",
                    line_no,
                    start_col + i,
                    line_no,
                    start_col + run_end,
                    chars[i..run_end].iter().collect(),
                );
                self.push(d, Some(cur));
                data_start = run_end;
                i = run_end;
                continue;
            }
            if c == '`'
                && let Some(end) =
                    self.try_code_span(chars, i, line_no, start_col, cur, &mut data_start)
            {
                i = end;
                continue;
            }
            if c == '<' {
                if let Some(end) =
                    self.try_autolink(chars, i, line_no, start_col, cur, &mut data_start)
                {
                    i = end;
                    continue;
                }
                if let Some(end) =
                    self.try_html_text(chars, i, line_no, start_col, cur, &mut data_start)
                {
                    i = end;
                    continue;
                }
            }
            if (c == '[' && suppressed_link_at != Some(i))
                || (c == '!' && chars.get(i + 1) == Some(&'['))
            {
                if let Some(end) =
                    self.try_link_image(chars, i, line_no, start_col, cur, &mut data_start)
                {
                    i = end;
                    continue;
                }
                if c == '!' {
                    // `![` was consumed as a single (failed) image label start;
                    // micromark does not re-attempt a link at the `[`.
                    suppressed_link_at = Some(i + 1);
                }
            }
            if (c == 'h' || c == 'H' || c == 'w' || c == 'W')
                && is_url_boundary(chars, i)
                && let Some(end) =
                    self.try_literal_autolink(chars, i, line_no, start_col, cur, &mut data_start)
            {
                i = end;
                continue;
            }
            if (c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-'))
                && is_url_boundary(chars, i)
                && let Some(end) =
                    self.try_literal_email(chars, i, line_no, start_col, cur, &mut data_start)
            {
                i = end;
                continue;
            }
            i += 1;
        }
        self.flush_data(data_start, chars.len(), chars, line_no, start_col, cur);
    }

    fn flush_data(
        &mut self,
        from: usize,
        to: usize,
        chars: &[char],
        line_no: usize,
        start_col: usize,
        parent: usize,
    ) {
        if to > from {
            let t = self.tok(
                "data",
                line_no,
                start_col + from,
                line_no,
                start_col + to,
                chars[from..to].iter().collect(),
            );
            self.push(t, Some(parent));
        }
    }

    fn try_code_span(
        &mut self,
        chars: &[char],
        i: usize,
        line_no: usize,
        start_col: usize,
        parent: usize,
        data_start: &mut usize,
    ) -> Option<usize> {
        let mut n = 0;
        while i + n < chars.len() && chars[i + n] == '`' {
            n += 1;
        }
        let mut j = i + n;
        let mut found = None;
        while j < chars.len() {
            if chars[j] == '`' {
                let mut m = 0;
                while j + m < chars.len() && chars[j + m] == '`' {
                    m += 1;
                }
                if m == n {
                    found = Some(j);
                    break;
                }
                j += m;
            } else {
                j += 1;
            }
        }
        let close = found?;
        self.flush_data(*data_start, i, chars, line_no, start_col, parent);
        let end = close + n;
        let ct = self.tok(
            "codeText",
            line_no,
            start_col + i,
            line_no,
            start_col + end,
            chars[i..end].iter().collect(),
        );
        let ctidx = self.push(ct, Some(parent));
        let s1 = self.tok(
            "codeTextSequence",
            line_no,
            start_col + i,
            line_no,
            start_col + i + n,
            chars[i..i + n].iter().collect(),
        );
        self.push(s1, Some(ctidx));
        if close > i + n {
            let cs = i + n;
            let content = &chars[cs..close];
            // CommonMark: if content begins and ends with a space and is not
            // all spaces, one space is stripped from each side as padding.
            let has_pad = content.len() >= 2
                && content[0] == ' '
                && content[content.len() - 1] == ' '
                && content.iter().any(|c| *c != ' ');
            if has_pad {
                let p1 = self.tok(
                    "codeTextPadding",
                    line_no,
                    start_col + cs,
                    line_no,
                    start_col + cs + 1,
                    " ".into(),
                );
                self.push(p1, Some(ctidx));
                if close - 1 > cs + 1 {
                    let d = self.tok(
                        "codeTextData",
                        line_no,
                        start_col + cs + 1,
                        line_no,
                        start_col + close - 1,
                        chars[cs + 1..close - 1].iter().collect(),
                    );
                    self.push(d, Some(ctidx));
                }
                let p2 = self.tok(
                    "codeTextPadding",
                    line_no,
                    start_col + close - 1,
                    line_no,
                    start_col + close,
                    " ".into(),
                );
                self.push(p2, Some(ctidx));
            } else {
                let d = self.tok(
                    "codeTextData",
                    line_no,
                    start_col + cs,
                    line_no,
                    start_col + close,
                    chars[cs..close].iter().collect(),
                );
                self.push(d, Some(ctidx));
            }
        }
        let s2 = self.tok(
            "codeTextSequence",
            line_no,
            start_col + close,
            line_no,
            start_col + end,
            chars[close..end].iter().collect(),
        );
        self.push(s2, Some(ctidx));
        *data_start = end;
        Some(end)
    }

    fn try_autolink(
        &mut self,
        chars: &[char],
        i: usize,
        line_no: usize,
        start_col: usize,
        parent: usize,
        data_start: &mut usize,
    ) -> Option<usize> {
        // <scheme:...> or <email>
        let mut j = i + 1;
        let mut inner = String::new();
        while j < chars.len() && chars[j] != '>' && chars[j] != '<' && !chars[j].is_whitespace() {
            inner.push(chars[j]);
            j += 1;
        }
        if j >= chars.len() || chars[j] != '>' || inner.is_empty() {
            return None;
        }
        let is_uri = uri_scheme_re().is_match(&inner);
        let is_email = !is_uri && email_re().is_match(&inner);
        if !is_uri && !is_email {
            return None;
        }
        self.flush_data(*data_start, i, chars, line_no, start_col, parent);
        let end = j + 1;
        let al = self.tok(
            "autolink",
            line_no,
            start_col + i,
            line_no,
            start_col + end,
            chars[i..end].iter().collect(),
        );
        let alidx = self.push(al, Some(parent));
        let m1 = self.tok(
            "autolinkMarker",
            line_no,
            start_col + i,
            line_no,
            start_col + i + 1,
            "<".into(),
        );
        self.push(m1, Some(alidx));
        let proto_kind = if is_email {
            "autolinkEmail"
        } else {
            "autolinkProtocol"
        };
        let proto = self.tok(
            proto_kind,
            line_no,
            start_col + i + 1,
            line_no,
            start_col + j,
            inner,
        );
        self.push(proto, Some(alidx));
        let m2 = self.tok(
            "autolinkMarker",
            line_no,
            start_col + j,
            line_no,
            start_col + end,
            ">".into(),
        );
        self.push(m2, Some(alidx));
        *data_start = end;
        Some(end)
    }

    fn try_html_text(
        &mut self,
        chars: &[char],
        i: usize,
        line_no: usize,
        start_col: usize,
        parent: usize,
        data_start: &mut usize,
    ) -> Option<usize> {
        let rest: String = chars[i..].iter().collect();
        // HTML comment
        let end_off = if rest.starts_with("<!--") {
            rest.find("-->").map(|p| p + 3)
        } else {
            html_tag_re()
                .find(&rest)
                .filter(|m| m.start() == 0)
                .map(|m| m.as_str().chars().count())
        }?;
        self.flush_data(*data_start, i, chars, line_no, start_col, parent);
        let end = i + end_off;
        let t = self.tok(
            "htmlText",
            line_no,
            start_col + i,
            line_no,
            start_col + end,
            chars[i..end].iter().collect(),
        );
        self.push(t, Some(parent));
        *data_start = end;
        Some(end)
    }

    fn try_literal_autolink(
        &mut self,
        chars: &[char],
        i: usize,
        line_no: usize,
        start_col: usize,
        parent: usize,
        data_start: &mut usize,
    ) -> Option<usize> {
        let rest: String = chars[i..].iter().collect();
        let m = literal_url_re().find(&rest).filter(|m| m.start() == 0)?;
        let mut matched: Vec<char> = m.as_str().chars().collect();
        // GFM trailing punctuation trimming.
        trim_url_trailing(&mut matched);
        if matched.is_empty() {
            return None;
        }
        let www = matched[0] == 'w' || matched[0] == 'W';
        let text: String = matched.iter().collect();
        // Must look like a real URL (has a dot).
        if www && !text.contains('.') {
            return None;
        }
        self.flush_data(*data_start, i, chars, line_no, start_col, parent);
        let end = i + matched.len();
        let la = self.tok(
            "literalAutolink",
            line_no,
            start_col + i,
            line_no,
            start_col + end,
            text.clone(),
        );
        let laidx = self.push(la, Some(parent));
        let kind = if www {
            "literalAutolinkWww"
        } else {
            "literalAutolinkHttp"
        };
        let child = self.tok(kind, line_no, start_col + i, line_no, start_col + end, text);
        self.push(child, Some(laidx));
        *data_start = end;
        Some(end)
    }

    fn try_literal_email(
        &mut self,
        chars: &[char],
        i: usize,
        line_no: usize,
        start_col: usize,
        parent: usize,
        data_start: &mut usize,
    ) -> Option<usize> {
        let rest: String = chars[i..].iter().collect();
        let m = literal_email_re().find(&rest).filter(|m| m.start() == 0)?;
        let matched: Vec<char> = m.as_str().chars().collect();
        // Must be followed by a non-word boundary.
        let after = chars.get(i + matched.len());
        if let Some(a) = after
            && (a.is_alphanumeric() || *a == '@' || *a == '-')
        {
            return None;
        }
        self.flush_data(*data_start, i, chars, line_no, start_col, parent);
        let end = i + matched.len();
        let text: String = matched.iter().collect();
        let la = self.tok(
            "literalAutolink",
            line_no,
            start_col + i,
            line_no,
            start_col + end,
            text.clone(),
        );
        let laidx = self.push(la, Some(parent));
        let child = self.tok(
            "literalAutolinkEmail",
            line_no,
            start_col + i,
            line_no,
            start_col + end,
            text,
        );
        self.push(child, Some(laidx));
        *data_start = end;
        Some(end)
    }

    #[allow(clippy::too_many_arguments)]
    fn try_link_image(
        &mut self,
        chars: &[char],
        i: usize,
        line_no: usize,
        start_col: usize,
        parent: usize,
        data_start: &mut usize,
    ) -> Option<usize> {
        let image = chars[i] == '!';
        let br_open = if image { i + 1 } else { i };
        // Find matching closing bracket at depth 0.
        let mut depth = 0i32;
        let mut j = br_open;
        let mut rb = None;
        while j < chars.len() {
            match chars[j] {
                '\\' => {
                    j += 2;
                    continue;
                }
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        rb = Some(j);
                        break;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        let rb = rb?;
        // Determine what follows: resource '(', reference '[', or shortcut.
        let after = rb + 1;
        enum Follow {
            Inline,
            Reference(usize, usize),
            Shortcut,
        }
        let (end, follow) = if chars.get(after) == Some(&'(') {
            let close = find_paren_close(chars, after)?;
            (close + 1, Follow::Inline)
        } else if chars.get(after) == Some(&'[') {
            // reference [label][ref]
            let mut k = after + 1;
            while k < chars.len() && chars[k] != ']' {
                if chars[k] == '\\' {
                    k += 1;
                }
                k += 1;
            }
            if k >= chars.len() {
                return None;
            }
            (k + 1, Follow::Reference(after, k))
        } else {
            // Shortcut reference: `[text]` is a link only if `text` is a
            // defined label; otherwise it is literal text, recorded as an
            // undefined shortcut use (micromark's undefinedReferenceShortcut).
            let label: String = chars[br_open + 1..rb].iter().collect();
            if !self.defined_labels.contains(&normalize_label(&label)) {
                if !label.trim().is_empty() && !label.contains(']') {
                    self.tree
                        .undefined_shortcuts
                        .push(super::tokens::UndefinedShortcut {
                            label,
                            line: line_no,
                            column: start_col + i,
                            length: rb + 1 - i,
                        });
                }
                return None;
            }
            (rb + 1, Follow::Shortcut)
        };

        self.flush_data(*data_start, i, chars, line_no, start_col, parent);
        let node_kind = if image { "image" } else { "link" };
        let node = self.tok(
            node_kind,
            line_no,
            start_col + i,
            line_no,
            start_col + end,
            chars[i..end].iter().collect(),
        );
        let nidx = self.push(node, Some(parent));

        // label
        let label = self.tok(
            "label",
            line_no,
            start_col + i,
            line_no,
            start_col + rb + 1,
            chars[i..rb + 1].iter().collect(),
        );
        let lidx = self.push(label, Some(nidx));
        if image {
            let im = self.tok(
                "labelImageMarker",
                line_no,
                start_col + i,
                line_no,
                start_col + i + 1,
                "!".into(),
            );
            self.push(im, Some(lidx));
        }
        let lm1 = self.tok(
            "labelMarker",
            line_no,
            start_col + br_open,
            line_no,
            start_col + br_open + 1,
            "[".into(),
        );
        self.push(lm1, Some(lidx));
        let text_from = br_open + 1;
        let lt = self.tok(
            "labelText",
            line_no,
            start_col + text_from,
            line_no,
            start_col + rb,
            chars[text_from..rb].iter().collect(),
        );
        let ltidx = self.push(lt, Some(lidx));
        if rb > text_from {
            let d = self.tok(
                "data",
                line_no,
                start_col + text_from,
                line_no,
                start_col + rb,
                chars[text_from..rb].iter().collect(),
            );
            self.push(d, Some(ltidx));
        }
        let lm2 = self.tok(
            "labelMarker",
            line_no,
            start_col + rb,
            line_no,
            start_col + rb + 1,
            "]".into(),
        );
        self.push(lm2, Some(lidx));

        if let Follow::Reference(ref_open, ref_close) = follow {
            // reference token
            let reference = self.tok(
                "reference",
                line_no,
                start_col + ref_open,
                line_no,
                start_col + ref_close + 1,
                chars[ref_open..ref_close + 1].iter().collect(),
            );
            let ridx = self.push(reference, Some(nidx));
            let rm1 = self.tok(
                "referenceMarker",
                line_no,
                start_col + ref_open,
                line_no,
                start_col + ref_open + 1,
                "[".into(),
            );
            self.push(rm1, Some(ridx));
            if ref_close > ref_open + 1 {
                let rs = self.tok(
                    "referenceString",
                    line_no,
                    start_col + ref_open + 1,
                    line_no,
                    start_col + ref_close,
                    chars[ref_open + 1..ref_close].iter().collect(),
                );
                let rsidx = self.push(rs, Some(ridx));
                let d = self.tok(
                    "data",
                    line_no,
                    start_col + ref_open + 1,
                    line_no,
                    start_col + ref_close,
                    chars[ref_open + 1..ref_close].iter().collect(),
                );
                self.push(d, Some(rsidx));
            }
            let rm2 = self.tok(
                "referenceMarker",
                line_no,
                start_col + ref_close,
                line_no,
                start_col + ref_close + 1,
                "]".into(),
            );
            self.push(rm2, Some(ridx));
        } else if let Follow::Inline = follow {
            // inline resource ( dest "title" )
            let res_open = after;
            let res_close = end - 1;
            self.emit_resource(chars, res_open, res_close, line_no, start_col, nidx);
        }
        // Follow::Shortcut emits only the label (no reference/resource).
        *data_start = end;
        Some(end)
    }

    fn emit_resource(
        &mut self,
        chars: &[char],
        open: usize,
        close: usize,
        line_no: usize,
        start_col: usize,
        parent: usize,
    ) {
        let resource = self.tok(
            "resource",
            line_no,
            start_col + open,
            line_no,
            start_col + close + 1,
            chars[open..close + 1].iter().collect(),
        );
        let residx = self.push(resource, Some(parent));
        let m1 = self.tok(
            "resourceMarker",
            line_no,
            start_col + open,
            line_no,
            start_col + open + 1,
            "(".into(),
        );
        self.push(m1, Some(residx));
        // parse destination
        let mut p = open + 1;
        while p < close && (chars[p] == ' ' || chars[p] == '\t') {
            p += 1;
        }
        let mut dest_start = p;
        let mut dest_end;
        if p < close && chars[p] == '<' {
            // <dest>
            dest_start = p + 1;
            let mut q = dest_start;
            while q < close && chars[q] != '>' {
                q += 1;
            }
            dest_end = q;
        } else {
            let mut q = p;
            while q < close && !chars[q].is_whitespace() {
                q += 1;
            }
            dest_end = q;
        }
        dest_end = dest_end.min(close);
        if dest_end > dest_start {
            let dest = self.tok(
                "resourceDestination",
                line_no,
                start_col + dest_start,
                line_no,
                start_col + dest_end,
                chars[dest_start..dest_end].iter().collect(),
            );
            let didx = self.push(dest, Some(residx));
            let raw = self.tok(
                "resourceDestinationRaw",
                line_no,
                start_col + dest_start,
                line_no,
                start_col + dest_end,
                chars[dest_start..dest_end].iter().collect(),
            );
            let ridx = self.push(raw, Some(didx));
            let s = self.tok(
                "resourceDestinationString",
                line_no,
                start_col + dest_start,
                line_no,
                start_col + dest_end,
                chars[dest_start..dest_end].iter().collect(),
            );
            let sidx = self.push(s, Some(ridx));
            let d = self.tok(
                "data",
                line_no,
                start_col + dest_start,
                line_no,
                start_col + dest_end,
                chars[dest_start..dest_end].iter().collect(),
            );
            self.push(d, Some(sidx));
        }
        let m2 = self.tok(
            "resourceMarker",
            line_no,
            start_col + close,
            line_no,
            start_col + close + 1,
            ")".into(),
        );
        self.push(m2, Some(residx));
    }

    /// Recompute a container token's `text` from its start/end using the raw
    /// source lines (needed for htmlFlow comment detection and code fence
    /// text). Coarse: joins source lines in the span.
    fn fix_text(&mut self, idx: usize) {
        let (sl, sc, el, ec) = {
            let t = &self.tree.tokens[idx];
            (t.start_line, t.start_column, t.end_line, t.end_column)
        };
        let mut text = String::new();
        for line in sl..=el {
            let lc = &self.lines[line - 1].chars;
            let from = if line == sl { sc - 1 } else { 0 };
            let to = if line == el {
                (ec - 1).min(lc.len())
            } else {
                lc.len()
            };
            if line > sl {
                text.push('\n');
            }
            if from <= lc.len() {
                let to = to.max(from);
                text.push_str(&lc[from..to.min(lc.len())].iter().collect::<String>());
            }
        }
        self.tree.tokens[idx].text = text;
    }
}

fn detect_autolink(chars: &[char], i: usize) -> Option<usize> {
    let mut j = i + 1;
    let mut inner = String::new();
    while j < chars.len() && chars[j] != '>' && chars[j] != '<' && !chars[j].is_whitespace() {
        inner.push(chars[j]);
        j += 1;
    }
    if j >= chars.len() || chars[j] != '>' || inner.is_empty() {
        return None;
    }
    if uri_scheme_re().is_match(&inner) || email_re().is_match(&inner) {
        Some(j + 1)
    } else {
        None
    }
}

fn detect_html(chars: &[char], i: usize) -> Option<usize> {
    let rest: String = chars[i..].iter().collect();
    if rest.starts_with("<!--") {
        rest.find("-->").map(|p| i + p + 3)
    } else {
        html_tag_re()
            .find(&rest)
            .filter(|m| m.start() == 0)
            .map(|m| i + m.as_str().chars().count())
    }
}

fn detect_link(chars: &[char], i: usize) -> Option<usize> {
    let image = chars[i] == '!';
    let br_open = if image { i + 1 } else { i };
    let mut depth = 0i32;
    let mut j = br_open;
    let mut rb = None;
    while j < chars.len() {
        match chars[j] {
            '\\' => {
                j += 2;
                continue;
            }
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    rb = Some(j);
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    let rb = rb?;
    let after = rb + 1;
    if chars.get(after) == Some(&'(') {
        find_paren_close(chars, after).map(|c| c + 1)
    } else if chars.get(after) == Some(&'[') {
        let mut k = after + 1;
        while k < chars.len() && chars[k] != ']' {
            if chars[k] == '\\' {
                k += 1;
            }
            k += 1;
        }
        if k >= chars.len() { None } else { Some(k + 1) }
    } else {
        None
    }
}

fn detect_literal(chars: &[char], i: usize) -> Option<usize> {
    let rest: String = chars[i..].iter().collect();
    if let Some(m) = literal_url_re().find(&rest).filter(|m| m.start() == 0) {
        let mut matched: Vec<char> = m.as_str().chars().collect();
        trim_url_trailing(&mut matched);
        if !matched.is_empty() {
            return Some(i + matched.len());
        }
    }
    if let Some(m) = literal_email_re().find(&rest).filter(|m| m.start() == 0) {
        let len = m.as_str().chars().count();
        return Some(i + len);
    }
    None
}

/// A resolved emphasis/strong span (marker character positions).
#[derive(Clone, Copy)]
pub struct EmphSpan {
    pub open_start: usize,
    pub open_len: usize,
    pub close_start: usize,
    pub close_len: usize,
    pub ch: char,
}

struct DelimRun {
    start: usize,
    orig: usize,
    remaining: usize,
    consumed_start: usize, // chars consumed from the start (for closers)
    ch: char,
    can_open: bool,
    can_close: bool,
}

fn is_md_punct(c: char) -> bool {
    c.is_ascii_punctuation()
}

/// Resolve emphasis/strong spans over `chars`, ignoring delimiters inside the
/// given masked (construct) ranges. Implements a pragmatic version of the
/// CommonMark delimiter-run algorithm (sufficient for common, well-nested
/// emphasis).
pub fn emphasis_spans(chars: &[char], masked: &[(usize, usize)]) -> Vec<EmphSpan> {
    let is_masked = |i: usize| masked.iter().any(|&(a, b)| i >= a && i < b);
    let mut runs: Vec<DelimRun> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' {
            i += 2;
            continue;
        }
        if (chars[i] == '*' || chars[i] == '_') && !is_masked(i) {
            let ch = chars[i];
            let start = i;
            let mut n = 0;
            while i < chars.len() && chars[i] == ch && !is_masked(i) {
                n += 1;
                i += 1;
            }
            let before = if start == 0 { ' ' } else { chars[start - 1] };
            let after = if i >= chars.len() { ' ' } else { chars[i] };
            let before_ws = before.is_whitespace();
            let after_ws = after.is_whitespace();
            let before_punct = is_md_punct(before);
            let after_punct = is_md_punct(after);
            let left_flanking = !after_ws && (!after_punct || before_ws || before_punct);
            let right_flanking = !before_ws && (!before_punct || after_ws || after_punct);
            let (can_open, can_close) = if ch == '_' {
                (
                    left_flanking && (!right_flanking || before_punct),
                    right_flanking && (!left_flanking || after_punct),
                )
            } else {
                (left_flanking, right_flanking)
            };
            runs.push(DelimRun {
                start,
                orig: n,
                remaining: n,
                consumed_start: 0,
                ch,
                can_open,
                can_close,
            });
            continue;
        }
        i += 1;
    }

    let mut spans = Vec::new();
    // Process closers left to right, matching the nearest compatible opener.
    for c in 0..runs.len() {
        if !runs[c].can_close {
            continue;
        }
        loop {
            if runs[c].remaining == 0 {
                break;
            }
            // find opener
            let mut o = None;
            let mut k = c;
            while k > 0 {
                k -= 1;
                if runs[k].can_open && runs[k].ch == runs[c].ch && runs[k].remaining > 0 {
                    // rule of three
                    let both_multiple_of_3 =
                        runs[k].orig.is_multiple_of(3) && runs[c].orig.is_multiple_of(3);
                    let sum_multiple_of_3 = (runs[k].orig + runs[c].orig).is_multiple_of(3);
                    let blocked = (runs[c].can_open || runs[k].can_close)
                        && sum_multiple_of_3
                        && !both_multiple_of_3;
                    if !blocked {
                        o = Some(k);
                        break;
                    }
                }
            }
            let Some(o) = o else { break };
            let use_len = if runs[o].remaining >= 2 && runs[c].remaining >= 2 {
                2
            } else {
                1
            };
            let open_start = runs[o].start + runs[o].remaining - use_len;
            let close_start = runs[c].start + runs[c].consumed_start;
            spans.push(EmphSpan {
                open_start,
                open_len: use_len,
                close_start,
                close_len: use_len,
                ch: runs[c].ch,
            });
            runs[o].remaining -= use_len;
            runs[c].remaining -= use_len;
            runs[c].consumed_start += use_len;
        }
    }
    spans
}

fn uri_scheme_re() -> &'static regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^[A-Za-z][A-Za-z0-9+.\-]{1,31}:").unwrap())
}

fn email_re() -> &'static regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"^[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?)*$",
        )
        .unwrap()
    })
}

fn html_tag_re() -> &'static regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    // Open/close tag (attributes without '>' ). A simplification of the
    // CommonMark raw-HTML production sufficient for the rules that use it.
    RE.get_or_init(|| regex::Regex::new(r#"^</?[A-Za-z][A-Za-z0-9-]*(?:\s+[^<>]*?)?/?>"#).unwrap())
}

fn literal_url_re() -> &'static regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^(?i)(?:https?://|www\.)[^\s<]*").unwrap())
}

fn literal_email_re() -> &'static regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    // GFM email autolink (simplified).
    RE.get_or_init(|| {
        regex::Regex::new(
            r"^[A-Za-z0-9._%+-]+@[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)+",
        )
        .unwrap()
    })
}

/// Whether a literal autolink may start at `i` (GFM boundary: start of text
/// or preceded by a non-word character).
fn is_url_boundary(chars: &[char], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let p = chars[i - 1];
    !(p.is_alphanumeric() || p == '/' || p == ':' || p == '.')
}

/// GFM trailing-punctuation trimming for literal autolinks.
fn trim_url_trailing(matched: &mut Vec<char>) {
    while let Some(&last) = matched.last() {
        if matches!(
            last,
            '?' | '!' | '.' | ',' | ':' | '*' | '_' | '~' | ';' | '\'' | '"'
        ) {
            matched.pop();
            continue;
        }
        if last == ')' {
            let opens = matched.iter().filter(|&&c| c == '(').count();
            let closes = matched.iter().filter(|&&c| c == ')').count();
            if closes > opens {
                matched.pop();
                continue;
            }
        }
        break;
    }
}

/// Find the `)` closing the `(` at index `open`, honouring nesting and escapes.
fn find_paren_close(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut j = open;
    while j < chars.len() {
        match chars[j] {
            '\\' => {
                j += 2;
                continue;
            }
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

fn byte_off(chars: &[char], char_idx: usize) -> usize {
    chars.iter().take(char_idx).map(|c| c.len_utf8()).sum()
}

/// Split content into lines. Recognizes `\n`, `\r\n`, `\r`.
fn split_lines(content: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let chars: Vec<char> = content.chars().collect();
    let mut cur = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            lines.push(Line {
                chars: std::mem::take(&mut cur),
                has_newline: true,
            });
            i += 1;
        } else if c == '\r' {
            let has = true;
            lines.push(Line {
                chars: std::mem::take(&mut cur),
                has_newline: has,
            });
            if i + 1 < chars.len() && chars[i + 1] == '\n' {
                i += 2;
            } else {
                i += 1;
            }
        } else {
            cur.push(c);
            i += 1;
        }
    }
    // trailing content without newline
    if !cur.is_empty() {
        lines.push(Line {
            chars: cur,
            has_newline: false,
        });
    } else if content.is_empty() {
        // empty input -> no lines
    }
    lines
}

// --- classification helpers ---

fn atx_level(trimmed: &str) -> Option<usize> {
    let chars: Vec<char> = trimmed.chars().collect();
    let mut n = 0;
    while n < chars.len() && chars[n] == '#' {
        n += 1;
    }
    if n == 0 || n > 6 {
        return None;
    }
    // must be followed by space/tab or end
    if n == chars.len() || chars[n] == ' ' || chars[n] == '\t' {
        Some(n)
    } else {
        None
    }
}

/// Returns (fence_char, fence_len) if the line opens/closes a fenced code block.
fn fenced_fence(trimmed: &str) -> Option<(char, usize)> {
    let chars: Vec<char> = trimmed.chars().collect();
    let first = *chars.first()?;
    if first != '`' && first != '~' {
        return None;
    }
    let mut n = 0;
    while n < chars.len() && chars[n] == first {
        n += 1;
    }
    if n < 3 {
        return None;
    }
    // backtick fences cannot contain backticks in the info string
    if first == '`' && chars[n..].contains(&'`') {
        return None;
    }
    Some((first, n))
}

fn only_fence(trimmed: &str, fence_char: char) -> bool {
    trimmed
        .chars()
        .all(|c| c == fence_char || c == ' ' || c == '\t')
}

fn is_thematic_break(trimmed: &str) -> bool {
    let t: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    if t.len() < 3 {
        return false;
    }
    let first = t.chars().next().unwrap();
    if first != '-' && first != '*' && first != '_' {
        return false;
    }
    t.chars().all(|c| c == first)
}

fn is_setext_underline(trimmed: &str) -> bool {
    let t = trimmed.trim_end();
    if t.is_empty() {
        return false;
    }
    let first = t.chars().next().unwrap();
    if first != '=' && first != '-' {
        return false;
    }
    t.chars().all(|c| c == first)
}

/// Returns (ordered, marker_char, marker_len_in_chars) if the trimmed line
/// starts a list item. marker_len includes the digits+delimiter for ordered.
fn list_marker(trimmed: &str) -> Option<(bool, char, usize)> {
    let chars: Vec<char> = trimmed.chars().collect();
    let first = *chars.first()?;
    if first == '-' || first == '+' || first == '*' {
        // must be followed by space/tab or end, and not a thematic break
        if chars.len() == 1 || chars[1] == ' ' || chars[1] == '\t' {
            if is_thematic_break(trimmed) {
                return None;
            }
            return Some((false, first, 1));
        }
        return None;
    }
    if first.is_ascii_digit() {
        let mut n = 0;
        while n < chars.len() && chars[n].is_ascii_digit() {
            n += 1;
        }
        if n > 9 {
            return None;
        }
        if n < chars.len()
            && (chars[n] == '.' || chars[n] == ')')
            && (n + 1 == chars.len() || chars[n + 1] == ' ' || chars[n + 1] == '\t')
        {
            return Some((true, chars[n], n + 1));
        }
    }
    None
}

/// A list marker that can interrupt a paragraph: non-empty content and, for
/// ordered lists, starting number must be 1.
fn interrupts_paragraph_list(trimmed: &str) -> bool {
    if let Some((ordered, _c, mlen)) = list_marker(trimmed) {
        let chars: Vec<char> = trimmed.chars().collect();
        // content after marker + following whitespace
        let mut i = mlen;
        while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
            i += 1;
        }
        if i >= chars.len() {
            return false; // empty item does not interrupt
        }
        if ordered {
            // ordered list interrupts a paragraph only if it starts with 1
            let digits: String = chars.iter().take_while(|c| c.is_ascii_digit()).collect();
            return digits == "1";
        }
        return true;
    }
    false
}

/// Unescaped `|` positions in `lc` starting from `lead`.
fn dividers(lc: &[char], lead: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut i = lead;
    while i < lc.len() {
        if lc[i] == '\\' {
            i += 2;
            continue;
        }
        if lc[i] == '|' {
            out.push(i);
        }
        i += 1;
    }
    out
}

/// Cell spans (char index ranges) for a table row, honouring optional
/// leading/trailing pipes.
fn compute_cells(lc: &[char], lead: usize) -> Vec<(usize, usize)> {
    let positions = dividers(lc, lead);
    if positions.is_empty() {
        return vec![(lead, lc.len())];
    }
    let all_ws = |a: usize, b: usize| lc[a..b].iter().all(|c| *c == ' ' || *c == '\t');
    let leading = all_ws(lead, positions[0]);
    let last = *positions.last().unwrap();
    let trailing = all_ws(last + 1, lc.len());
    let mut starts = Vec::new();
    if !leading {
        starts.push(lead);
    }
    for (idx, &pos) in positions.iter().enumerate() {
        if trailing && idx == positions.len() - 1 {
            continue;
        }
        starts.push(pos);
    }
    let mut cells = Vec::new();
    for i in 0..starts.len() {
        let cs = starts[i];
        let ce = if i + 1 < starts.len() {
            starts[i + 1]
        } else {
            lc.len()
        };
        cells.push((cs, ce));
    }
    cells
}

fn count_cells(lc: &[char]) -> usize {
    let lead = leading_spaces(lc);
    compute_cells(lc, lead).len()
}

/// Trimmed inner content of a cell span (pipes and whitespace removed).
fn cell_content(lc: &[char], cs: usize, ce: usize) -> String {
    let mut p = cs;
    if p < ce && lc[p] == '|' {
        p += 1;
    }
    let mut e = ce;
    // strip trailing ws
    while e > p && (lc[e - 1] == ' ' || lc[e - 1] == '\t') {
        e -= 1;
    }
    if e > p && lc[e - 1] == '|' {
        e -= 1;
    }
    while e > p && (lc[e - 1] == ' ' || lc[e - 1] == '\t') {
        e -= 1;
    }
    while p < e && (lc[p] == ' ' || lc[p] == '\t') {
        p += 1;
    }
    lc[p..e.max(p)].iter().collect()
}

/// If `lc` is a valid GFM table delimiter row, return the number of columns.
fn delimiter_row_cells(lc: &[char]) -> Option<usize> {
    let lead = leading_spaces(lc);
    if lead >= 4 {
        return None;
    }
    if is_blank(lc) {
        return None;
    }
    let cells = compute_cells(lc, lead);
    if cells.is_empty() {
        return None;
    }
    let re = {
        use std::sync::OnceLock;
        static RE: OnceLock<regex::Regex> = OnceLock::new();
        RE.get_or_init(|| regex::Regex::new(r"^:?-+:?$").unwrap())
    };
    for &(cs, ce) in &cells {
        let content = cell_content(lc, cs, ce);
        if !re.is_match(&content) {
            return None;
        }
    }
    Some(cells.len())
}

/// Prefix width (marker + optional single space) for every line of a
/// blockquote, or `None` if any line lacks a `>` prefix (lazy continuation).
fn uniform_bq_prefixes(lines: &[Line], start: usize, end: usize) -> Option<Vec<usize>> {
    let mut widths = Vec::new();
    for line in &lines[start..=end] {
        let lc = &line.chars;
        let lead = leading_spaces(lc);
        if lead >= 4 || lc.get(lead) != Some(&'>') {
            return None;
        }
        let mut w = lead + 1;
        if lc.get(w) == Some(&' ') {
            w += 1;
        }
        widths.push(w);
    }
    Some(widths)
}

/// Normalize a reference label (lowercase, trim, collapse internal whitespace).
fn normalize_label(s: &str) -> String {
    let mut out = String::new();
    let mut prev_ws = false;
    for c in s.trim().chars() {
        if c.is_whitespace() {
            if !prev_ws {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            out.extend(c.to_lowercase());
            prev_ws = false;
        }
    }
    out
}

/// Collect normalized labels of all reference definitions in the document.
fn collect_definition_labels(lines: &[Line]) -> std::collections::HashSet<String> {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"^ {0,3}\[([^\]]+)\]:\s*\S").unwrap());
    let mut out = std::collections::HashSet::new();
    for line in lines {
        let s: String = line.chars.iter().collect();
        if let Some(c) = re.captures(&s) {
            out.insert(normalize_label(&c[1]));
        }
    }
    out
}

/// A link reference definition line: `[label]: destination`.
fn is_definition_line(trimmed: &str) -> bool {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"^\[[^\]]+\]:\s*\S").unwrap());
    re.is_match(trimmed)
}

fn is_html_block_start(trimmed: &str) -> bool {
    if !trimmed.starts_with('<') {
        return false;
    }
    let rest = &trimmed[1..];
    rest.starts_with('!')
        || rest.starts_with('?')
        || rest.starts_with('/')
        || rest
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic())
            .unwrap_or(false)
}
