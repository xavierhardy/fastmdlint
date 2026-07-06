//! Token tree model, mirroring the shape of markdownlint's micromark token
//! tree closely enough for the rules that consume it.
//!
//! Tokens are stored in an arena (`Vec<Token>`) in pre-order (document
//! order), which matches the order of micromark's flattened token list — the
//! order rules iterate in. Positions are 1-based lines and columns, matching
//! markdownlint's `startLine`/`startColumn`/`endLine`/`endColumn`.

/// A single token in the tree.
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: &'static str,
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub text: String,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
    pub in_html_flow: bool,
}

/// The parsed token tree.
#[derive(Debug, Default)]
pub struct Tree {
    /// Arena of tokens in document (pre-order) order.
    pub tokens: Vec<Token>,
    /// Indices of the top-level tokens.
    pub roots: Vec<usize>,
}

impl Tree {
    pub fn get(&self, idx: usize) -> &Token {
        &self.tokens[idx]
    }

    /// All tokens of any of the given kinds, in document order, excluding
    /// tokens inside an `htmlFlow` block (matching `filterByTypes` default).
    pub fn filter(&self, kinds: &[&str]) -> Vec<&Token> {
        self.tokens
            .iter()
            .filter(|t| kinds.contains(&t.kind) && !t.in_html_flow)
            .collect()
    }

    /// Like [`Tree::filter`] but returns arena indices.
    pub fn filter_idx(&self, kinds: &[&str]) -> Vec<usize> {
        self.tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| kinds.contains(&t.kind) && !t.in_html_flow)
            .map(|(i, _)| i)
            .collect()
    }

    /// Direct children of a token filtered by kind.
    pub fn children_of(&self, idx: usize, kinds: &[&str]) -> Vec<usize> {
        self.tokens[idx]
            .children
            .iter()
            .copied()
            .filter(|c| kinds.contains(&self.tokens[*c].kind))
            .collect()
    }

    /// First direct child matching one of the kinds.
    pub fn first_child(&self, idx: usize, kinds: &[&str]) -> Option<usize> {
        self.tokens[idx]
            .children
            .iter()
            .copied()
            .find(|c| kinds.contains(&self.tokens[*c].kind))
    }

    /// Nearest ancestor of one of the given kinds.
    pub fn parent_of_type(&self, idx: usize, kinds: &[&str]) -> Option<usize> {
        let mut cur = self.tokens[idx].parent;
        while let Some(p) = cur {
            if kinds.contains(&self.tokens[p].kind) {
                return Some(p);
            }
            cur = self.tokens[p].parent;
        }
        None
    }

    /// Descendants reached by walking a type path, as in
    /// `getDescendantsByType`. Each path element is a set of allowed kinds.
    pub fn descendants_by_type(&self, root: usize, path: &[&[&str]]) -> Vec<usize> {
        let mut current = vec![root];
        for step in path {
            let mut next = Vec::new();
            for &t in &current {
                for &c in &self.tokens[t].children {
                    if step.contains(&self.tokens[c].kind) {
                        next.push(c);
                    }
                }
            }
            current = next;
        }
        current
    }

    // --- Heading helpers (mirror micromark-helpers.cjs) ---

    /// Heading level for an `atxHeading`/`setextHeading` token.
    pub fn heading_level(&self, heading: usize) -> usize {
        let seq = self.tokens[heading]
            .children
            .iter()
            .copied()
            .find(|c| matches!(self.tokens[*c].kind, "atxHeadingSequence" | "setextHeadingLine"));
        let Some(seq) = seq else { return 1 };
        let text = &self.tokens[seq].text;
        let first = text.chars().next();
        match first {
            Some('#') => text.chars().count().min(6),
            Some('-') => 2,
            _ => 1,
        }
    }

    /// Heading style: "atx", "atx_closed", or "setext".
    pub fn heading_style(&self, heading: usize) -> &'static str {
        if self.tokens[heading].kind == "setextHeading" {
            return "setext";
        }
        let count = self.tokens[heading]
            .children
            .iter()
            .filter(|c| self.tokens[**c].kind == "atxHeadingSequence")
            .count();
        if count == 1 { "atx" } else { "atx_closed" }
    }

    /// Heading text content (data of atx/setext heading text, htmlText
    /// excluded, newlines collapsed to spaces).
    pub fn heading_text(&self, heading: usize) -> String {
        let text_tokens =
            self.descendants_by_type(heading, &[&["atxHeadingText", "setextHeadingText"]]);
        let mut out = String::new();
        for t in text_tokens {
            for &c in &self.tokens[t].children {
                if self.tokens[c].kind != "htmlText" {
                    out.push_str(&self.tokens[c].text);
                }
            }
        }
        out.replace(['\r', '\n'], " ")
    }
}

/// Token kinds that carry no textual content (used by MD027 and others).
pub const NON_CONTENT_TOKENS: &[&str] = &[
    "blockQuoteMarker",
    "blockQuotePrefix",
    "blockQuotePrefixWhitespace",
    "gfmFootnoteDefinitionIndent",
    "lineEnding",
    "lineEndingBlank",
    "linePrefix",
    "listItemIndent",
    "undefinedReference",
    "undefinedReferenceCollapsed",
    "undefinedReferenceFull",
    "undefinedReferenceShortcut",
];

/// Parsed HTML tag info from an `htmlText` token.
pub struct HtmlTagInfo {
    pub close: bool,
    pub name: String,
}

/// Mirror of `getHtmlTagInfo`.
pub fn html_tag_info(text: &str) -> Option<HtmlTagInfo> {
    // /^<([^!>][^/\s>]*)/
    let bytes: Vec<char> = text.chars().collect();
    if bytes.first() != Some(&'<') {
        return None;
    }
    let mut i = 1;
    if i >= bytes.len() {
        return None;
    }
    let first = bytes[i];
    if first == '!' || first == '>' {
        return None;
    }
    let mut name = String::new();
    name.push(first);
    i += 1;
    while i < bytes.len() {
        let c = bytes[i];
        if c == '/' || c.is_whitespace() || c == '>' {
            break;
        }
        name.push(c);
        i += 1;
    }
    let close = name.starts_with('/');
    Some(HtmlTagInfo {
        close,
        name: if close { name[1..].to_string() } else { name },
    })
}
