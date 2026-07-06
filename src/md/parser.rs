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
    chars.iter().take_while(|c| **c == ' ' || **c == '\t').count()
}

fn is_blank(chars: &[char]) -> bool {
    chars.iter().all(|c| *c == ' ' || *c == '\t')
}

impl Parser {
    pub fn parse(content: &str) -> Tree {
        let lines = split_lines(content);
        let mut p = Parser {
            lines,
            tree: Tree::default(),
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
            if let Some(_) = list_marker(&trimmed) {
                i = self.parse_list(i, None);
                continue;
            }
            if is_html_block_start(&trimmed) {
                i = self.parse_html_block(i, None);
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
            in_html_flow: false,
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
            let t = self.tok("lineEndingBlank", i + 1, len + 1, i + 2, 1, "\n".to_string());
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
        let end_line;
        let mut closed_at: Option<usize> = None;
        let mut j = start + 1;
        while j < self.lines.len() {
            let lc = &self.lines[j].chars;
            let jl = leading_spaces(lc);
            let jt: String = lc.iter().skip(jl).collect();
            if jl < 4 {
                if let Some((c, l)) = fenced_fence(&jt) {
                    if c == fence_char && l >= fence_len && only_fence(&jt, fence_char) {
                        closed_at = Some(j);
                        break;
                    }
                }
            }
            j += 1;
        }
        end_line = closed_at.unwrap_or(self.lines.len() - 1);
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
        let hf = self.tok("htmlFlow", sl, 1, end + 1, last_len + 1, text);
        let hidx = self.push(hf, parent);
        for k in start..=end {
            self.emit_line_ending(k, self.line_len(k) + 1, parent);
            let _ = k;
        }
        // mark children in_html_flow not needed (no children emitted)
        let _ = hidx;
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
        // Emit blockQuotePrefix + inner content as a single paragraph-ish block.
        // Build stripped inner lines and their column offsets.
        let mut inner_texts: Vec<(usize, usize, String)> = Vec::new(); // (line_no, offset_cols, text)
        for k in start..=end {
            let lc = self.lines[k].chars.clone();
            let lead = leading_spaces(&lc);
            if lead < 4 && lc.get(lead) == Some(&'>') {
                let mut off = lead + 1;
                // one optional following space
                if lc.get(off) == Some(&' ') {
                    off += 1;
                }
                inner_texts.push((k + 1, off, lc[off..].iter().collect()));
            } else {
                inner_texts.push((k + 1, 0, lc.iter().collect()));
            }
        }
        // content wrapper + paragraph (coarse) so inline rules can see text
        let content = self.tok(
            "content",
            sl,
            inner_texts[0].1 + 1,
            end + 1,
            last_len + 1,
            String::new(),
        );
        let cidx = self.push(content, Some(bqidx));
        let para = self.tok(
            "paragraph",
            sl,
            inner_texts[0].1 + 1,
            end + 1,
            last_len + 1,
            String::new(),
        );
        let pidx = self.push(para, Some(cidx));
        for (k, off, text) in &inner_texts {
            let chars: Vec<char> = text.chars().collect();
            if !chars.is_empty() {
                let data = self.tok(
                    "data",
                    *k,
                    off + 1,
                    *k,
                    off + chars.len() + 1,
                    text.clone(),
                );
                self.push(data, Some(pidx));
            }
        }
        self.emit_line_ending(end, last_len + 1, None);
        // Also emit a blockQuotePrefix token per line at top of blockQuote for
        // rules that look for it (MD027 etc.). Coarse: single prefix on line 1.
        end + 1
    }

    // --- list (coarse) ---
    fn parse_list(&mut self, start: usize, parent: Option<usize>) -> usize {
        let chars = self.lines[start].chars.clone();
        let lead = leading_spaces(&chars);
        let t: String = chars.iter().skip(lead).collect();
        let (ordered, first_marker, _mlen) = list_marker(&t).unwrap();
        let kind = if ordered { "listOrdered" } else { "listUnordered" };
        // Signature: ordered lists split on delimiter change, bullet lists on
        // marker-character change.
        let first_sig = (ordered, first_marker);
        // Collect list lines (same or deeper indent, until blank+dedent).
        let mut j = start;
        let mut end = start;
        while j < self.lines.len() {
            let lc = &self.lines[j].chars;
            if is_blank(lc) {
                // peek: continue if next non-blank is indented into the list
                let mut k = j + 1;
                while k < self.lines.len() && is_blank(&self.lines[k].chars) {
                    k += 1;
                }
                if k < self.lines.len() {
                    let klead = leading_spaces(&self.lines[k].chars);
                    let kt: String =
                        self.lines[k].chars.iter().skip(klead).collect();
                    if klead > lead || (klead == lead && list_marker(&kt).is_some()) {
                        j = k;
                        end = k;
                        continue;
                    }
                }
                break;
            }
            let jlead = leading_spaces(lc);
            let jt: String = lc.iter().skip(jlead).collect();
            if j == start {
                end = j;
                j += 1;
                continue;
            }
            if jlead == lead {
                if let Some((jord, jm, _)) = list_marker(&jt) {
                    if (jord, jm) != first_sig {
                        break; // marker change starts a new list
                    }
                    end = j;
                    j += 1;
                    continue;
                }
            }
            if jlead >= lead + 1 {
                end = j;
                j += 1;
            } else if jlead < 4
                && list_marker(&jt).is_none()
                && atx_level(&jt).is_none()
                && fenced_fence(&jt).is_none()
                && !is_thematic_break(&jt)
            {
                // lazy paragraph continuation
                end = j;
                j += 1;
            } else {
                break;
            }
        }
        let sl = start + 1;
        let last_len = self.line_len(end);
        let list = self.tok(kind, sl, lead + 1, end + 1, last_len + 1, String::new());
        let lidx = self.push(list, parent);
        // Emit list items coarsely: each marker line -> listItemPrefix + content
        let mut k = start;
        while k <= end {
            let lc = self.lines[k].chars.clone();
            if is_blank(&lc) {
                self.emit_line_ending(k, self.line_len(k) + 1, Some(lidx));
                k += 1;
                continue;
            }
            let klead = leading_spaces(&lc);
            let kt: String = lc.iter().skip(klead).collect();
            let line_no = k + 1;
            if let Some((ord, _m, mlen)) = list_marker(&kt) {
                // listItemPrefix
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
                    // listItemValue = digits
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
                // content
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
            } else {
                // continuation line: listItemIndent + content
                let indent_end = klead.min(lc.len());
                if indent_end > 0 {
                    let ind = self.tok(
                        "listItemIndent",
                        line_no,
                        1,
                        line_no,
                        indent_end + 1,
                        lc[..indent_end].iter().collect(),
                    );
                    self.push(ind, Some(lidx));
                }
            }
            self.emit_line_ending(k, self.line_len(k) + 1, Some(lidx));
            k += 1;
        }
        end + 1
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
            let lead = if k == start { first_lead } else { leading_spaces(&lc) };
            let line_no = k + 1;
            self.parse_inline(&lc[lead..], line_no, lead + 1, pidx);
            if k < end {
                let t = self.tok("lineEnding", line_no, lc.len() + 1, line_no + 1, 1, "\n".to_string());
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
            let lead = if k == start { first_lead } else { leading_spaces(&lc) };
            let line_no = k + 1;
            self.parse_inline(&lc[lead..], line_no, lead + 1, tidx);
            if k < end {
                let t = self.tok("lineEnding", line_no, lc.len() + 1, line_no + 1, 1, "\n".to_string());
                self.push(t, Some(tidx));
            }
        }
        // lineEnding between text and underline
        let te = self.tok("lineEnding", end + 1, text_last_len + 1, end + 2, 1, "\n".to_string());
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

    // --- inline (data + code spans) ---
    fn parse_inline(&mut self, chars: &[char], line_no: usize, start_col: usize, parent: usize) {
        // Emit `data` runs and `codeText` code spans.
        let mut i = 0;
        let mut data_start = 0;
        let flush = |p: &mut Parser, from: usize, to: usize, chars: &[char]| {
            if to > from {
                let t = p.tok(
                    "data",
                    line_no,
                    start_col + from,
                    line_no,
                    start_col + to,
                    chars[from..to].iter().collect(),
                );
                p.push(t, Some(parent));
            }
        };
        while i < chars.len() {
            if chars[i] == '`' {
                // count backticks
                let mut n = 0;
                while i + n < chars.len() && chars[i + n] == '`' {
                    n += 1;
                }
                // find closing run of exactly n backticks
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
                    flush(self, data_start, i, chars);
                    let end = close + n;
                    let ct = self.tok(
                        "codeText",
                        line_no,
                        start_col + i,
                        line_no,
                        start_col + end,
                        chars[i..end].iter().collect(),
                    );
                    let ctidx = self.push(ct, parent_opt(parent));
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
                        let d = self.tok(
                            "codeTextData",
                            line_no,
                            start_col + i + n,
                            line_no,
                            start_col + close,
                            chars[i + n..close].iter().collect(),
                        );
                        self.push(d, Some(ctidx));
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
                    i = end;
                    data_start = end;
                    continue;
                }
            }
            i += 1;
        }
        flush(self, data_start, chars.len(), chars);
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
            let to = if line == el { (ec - 1).min(lc.len()) } else { lc.len() };
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

fn parent_opt(p: usize) -> Option<usize> {
    Some(p)
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
    trimmed.chars().all(|c| c == fence_char || c == ' ' || c == '\t')
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
        if n < chars.len() && (chars[n] == '.' || chars[n] == ')') {
            if n + 1 == chars.len() || chars[n + 1] == ' ' || chars[n + 1] == '\t' {
                return Some((true, chars[n], n + 1));
            }
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

fn is_html_block_start(trimmed: &str) -> bool {
    if !trimmed.starts_with('<') {
        return false;
    }
    let rest = &trimmed[1..];
    rest.starts_with('!')
        || rest.starts_with('?')
        || rest.starts_with('/')
        || rest.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false)
}
