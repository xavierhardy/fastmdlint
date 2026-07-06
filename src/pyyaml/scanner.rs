//! A faithful port of PyYAML's `scanner.py` (MIT licensed).
//!
//! The port preserves PyYAML's exact token stream, including start/end marks
//! (character pointer, 0-based line and column), token values, scalar styles
//! and error messages, so the lint rules can rely on all of them.

use std::collections::{HashMap, VecDeque};

use super::error::{Mark, YamlError};
use super::pyrepr::py_repr_char;
use super::tokens::{DirectiveValue, Token, TokenKind};

type Result<T> = std::result::Result<T, YamlError>;

const Z: char = '\0';

#[inline]
fn in_z_break(ch: char) -> bool {
    matches!(ch, '\0' | '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

#[inline]
fn in_z_blank_break(ch: char) -> bool {
    matches!(
        ch,
        '\0' | ' ' | '\t' | '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

#[inline]
fn in_z_space_break(ch: char) -> bool {
    // '\0 \r\n\x85  ' (no tab)
    matches!(
        ch,
        '\0' | ' ' | '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

#[inline]
fn is_alnum(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'
}

#[derive(Debug, Clone)]
struct SimpleKey {
    token_number: usize,
    required: bool,
    index: usize,
    line: usize,
    mark: Mark,
}

pub struct Scanner {
    buffer: Vec<char>,
    pointer: usize,
    line: usize,
    column: usize,

    done: bool,
    flow_level: usize,
    tokens: VecDeque<Token>,
    tokens_taken: usize,
    indent: i64,
    indents: Vec<i64>,
    allow_simple_key: bool,
    possible_simple_keys: HashMap<usize, SimpleKey>,
    failed: bool,
}

impl Scanner {
    /// Create a scanner over an already-decoded string. A `'\0'` sentinel is
    /// appended, exactly like PyYAML's Reader.
    pub fn new(content: &str) -> Self {
        let mut buffer: Vec<char> = content.chars().collect();
        buffer.push(Z);
        let mut scanner = Self {
            buffer,
            pointer: 0,
            line: 0,
            column: 0,
            done: false,
            flow_level: 0,
            tokens: VecDeque::new(),
            tokens_taken: 0,
            indent: -1,
            indents: Vec::new(),
            allow_simple_key: true,
            possible_simple_keys: HashMap::new(),
            failed: false,
        };
        scanner.fetch_stream_start();
        scanner
    }

    /// Access to the underlying character buffer (with trailing `'\0'`).
    pub fn buffer(&self) -> &[char] {
        &self.buffer
    }

    pub fn into_buffer(self) -> Vec<char> {
        self.buffer
    }

    // --- Reader part -----------------------------------------------------

    #[inline]
    fn peek(&self, index: usize) -> char {
        self.buffer.get(self.pointer + index).copied().unwrap_or(Z)
    }

    #[inline]
    fn peek0(&self) -> char {
        self.peek(0)
    }

    fn prefix(&self, length: usize) -> String {
        let end = (self.pointer + length).min(self.buffer.len());
        self.buffer[self.pointer..end].iter().collect()
    }

    fn prefix_is(&self, s: &str) -> bool {
        s.chars().enumerate().all(|(i, ch)| self.peek(i) == ch)
    }

    fn forward(&mut self, length: usize) {
        for _ in 0..length {
            let ch = self.buffer.get(self.pointer).copied().unwrap_or(Z);
            self.pointer += 1;
            let next = self.buffer.get(self.pointer).copied();
            if matches!(ch, '\n' | '\u{85}' | '\u{2028}' | '\u{2029}')
                || (ch == '\r' && next != Some('\n'))
            {
                self.line += 1;
                self.column = 0;
            } else if ch != '\u{FEFF}' {
                self.column += 1;
            }
        }
    }

    fn forward1(&mut self) {
        self.forward(1);
    }

    pub fn get_mark(&self) -> Mark {
        Mark {
            pointer: self.pointer,
            line: self.line,
            column: self.column,
        }
    }

    fn error(
        &self,
        context: Option<&str>,
        context_mark: Option<Mark>,
        problem: String,
    ) -> YamlError {
        YamlError::new(context, context_mark, problem, Some(self.get_mark()))
    }

    // --- Public token API --------------------------------------------------

    /// Returns the next token, or `None` at the end of the stream. After an
    /// error, keeps returning the error.
    pub fn get_token(&mut self) -> Result<Option<Token>> {
        while self.need_more_tokens()? {
            self.fetch_more_tokens()?;
        }
        if let Some(token) = self.tokens.pop_front() {
            self.tokens_taken += 1;
            Ok(Some(token))
        } else {
            Ok(None)
        }
    }

    /// Returns a clone of the next token without consuming it, or `None` at
    /// the end of the stream.
    pub fn peek_token_cloned(&mut self) -> Result<Option<Token>> {
        while self.need_more_tokens()? {
            self.fetch_more_tokens()?;
        }
        Ok(self.tokens.front().cloned())
    }

    /// Collect all tokens until end of stream or first error (the error is
    /// returned alongside the tokens scanned so far).
    pub fn scan_all(content: &str) -> (Vec<Token>, Option<YamlError>, Vec<char>) {
        let mut scanner = Scanner::new(content);
        let mut tokens = Vec::new();
        let error = loop {
            match scanner.get_token() {
                Ok(Some(token)) => tokens.push(token),
                Ok(None) => break None,
                Err(e) => break Some(e),
            }
        };
        (tokens, error, scanner.into_buffer())
    }

    fn need_more_tokens(&mut self) -> Result<bool> {
        if self.failed {
            return Err(self.error(None, None, "scanner is in a failed state".into()));
        }
        if self.done {
            return Ok(false);
        }
        if self.tokens.is_empty() {
            return Ok(true);
        }
        // The current token may be a potential simple key, so we
        // need to look further.
        self.stale_possible_simple_keys()?;
        if self.next_possible_simple_key() == Some(self.tokens_taken) {
            return Ok(true);
        }
        Ok(false)
    }

    fn fetch_more_tokens(&mut self) -> Result<()> {
        match self.fetch_more_tokens_inner() {
            Ok(()) => Ok(()),
            Err(e) => {
                self.failed = true;
                Err(e)
            }
        }
    }

    fn fetch_more_tokens_inner(&mut self) -> Result<()> {
        // Eat whitespaces and comments until we reach the next token.
        self.scan_to_next_token();

        // Remove obsolete possible simple keys.
        self.stale_possible_simple_keys()?;

        // Compare the current indentation and column. It may add some tokens
        // and decrease the current indentation level.
        self.unwind_indent(self.column as i64);

        let ch = self.peek0();

        if ch == Z {
            return self.fetch_stream_end();
        }
        if ch == '%' && self.check_directive() {
            return self.fetch_directive();
        }
        if ch == '-' && self.check_document_start() {
            return self.fetch_document_indicator(TokenKind::DocumentStart);
        }
        if ch == '.' && self.check_document_end() {
            return self.fetch_document_indicator(TokenKind::DocumentEnd);
        }
        if ch == '[' {
            return self.fetch_flow_collection_start(TokenKind::FlowSequenceStart);
        }
        if ch == '{' {
            return self.fetch_flow_collection_start(TokenKind::FlowMappingStart);
        }
        if ch == ']' {
            return self.fetch_flow_collection_end(TokenKind::FlowSequenceEnd);
        }
        if ch == '}' {
            return self.fetch_flow_collection_end(TokenKind::FlowMappingEnd);
        }
        if ch == ',' {
            return self.fetch_flow_entry();
        }
        if ch == '-' && self.check_block_entry() {
            return self.fetch_block_entry();
        }
        if ch == '?' && self.check_key() {
            return self.fetch_key();
        }
        if ch == ':' && self.check_value() {
            return self.fetch_value();
        }
        if ch == '*' {
            return self.fetch_anchor(true);
        }
        if ch == '&' {
            return self.fetch_anchor(false);
        }
        if ch == '!' {
            return self.fetch_tag();
        }
        if ch == '|' && self.flow_level == 0 {
            return self.fetch_block_scalar('|');
        }
        if ch == '>' && self.flow_level == 0 {
            return self.fetch_block_scalar('>');
        }
        if ch == '\'' {
            return self.fetch_flow_scalar('\'');
        }
        if ch == '"' {
            return self.fetch_flow_scalar('"');
        }
        if self.check_plain() {
            return self.fetch_plain();
        }

        Err(self.error(
            Some("while scanning for the next token"),
            None,
            format!(
                "found character {} that cannot start any token",
                py_repr_char(ch)
            ),
        ))
    }

    // --- Simple keys -------------------------------------------------------

    fn next_possible_simple_key(&self) -> Option<usize> {
        self.possible_simple_keys
            .values()
            .map(|key| key.token_number)
            .min()
    }

    fn stale_possible_simple_keys(&mut self) -> Result<()> {
        let mut error: Option<YamlError> = None;
        let line = self.line;
        let index = self.pointer;
        let mark = self.get_mark();
        self.possible_simple_keys.retain(|_, key| {
            if key.line != line || index - key.index > 1024 {
                if key.required && error.is_none() {
                    error = Some(YamlError::new(
                        Some("while scanning a simple key"),
                        Some(key.mark),
                        "could not find expected ':'".into(),
                        Some(mark),
                    ));
                }
                false
            } else {
                true
            }
        });
        match error {
            Some(e) => {
                self.failed = true;
                Err(e)
            }
            None => Ok(()),
        }
    }

    fn save_possible_simple_key(&mut self) -> Result<()> {
        let required = self.flow_level == 0 && self.indent == self.column as i64;

        if self.allow_simple_key {
            self.remove_possible_simple_key()?;
            let token_number = self.tokens_taken + self.tokens.len();
            let key = SimpleKey {
                token_number,
                required,
                index: self.pointer,
                line: self.line,
                mark: self.get_mark(),
            };
            self.possible_simple_keys.insert(self.flow_level, key);
        }
        Ok(())
    }

    fn remove_possible_simple_key(&mut self) -> Result<()> {
        if let Some(key) = self.possible_simple_keys.get(&self.flow_level) {
            if key.required {
                let e = YamlError::new(
                    Some("while scanning a simple key"),
                    Some(key.mark),
                    "could not find expected ':'".into(),
                    Some(self.get_mark()),
                );
                self.failed = true;
                return Err(e);
            }
            self.possible_simple_keys.remove(&self.flow_level);
        }
        Ok(())
    }

    // --- Indentation -------------------------------------------------------

    fn unwind_indent(&mut self, column: i64) {
        // In the flow context, indentation is ignored.
        if self.flow_level != 0 {
            return;
        }

        while self.indent > column {
            let mark = self.get_mark();
            self.indent = self.indents.pop().unwrap_or(-1);
            self.tokens
                .push_back(Token::new(TokenKind::BlockEnd, mark, mark));
        }
    }

    fn add_indent(&mut self, column: i64) -> bool {
        if self.indent < column {
            self.indents.push(self.indent);
            self.indent = column;
            true
        } else {
            false
        }
    }

    // --- Fetchers ----------------------------------------------------------

    fn fetch_stream_start(&mut self) {
        let mark = self.get_mark();
        self.tokens
            .push_back(Token::new(TokenKind::StreamStart, mark, mark));
    }

    fn fetch_stream_end(&mut self) -> Result<()> {
        self.unwind_indent(-1);
        self.remove_possible_simple_key()?;
        self.allow_simple_key = false;
        self.possible_simple_keys.clear();

        let mark = self.get_mark();
        self.tokens
            .push_back(Token::new(TokenKind::StreamEnd, mark, mark));
        self.done = true;
        Ok(())
    }

    fn fetch_directive(&mut self) -> Result<()> {
        self.unwind_indent(-1);
        self.remove_possible_simple_key()?;
        self.allow_simple_key = false;

        let token = self.scan_directive()?;
        self.tokens.push_back(token);
        Ok(())
    }

    fn fetch_document_indicator(&mut self, kind: TokenKind) -> Result<()> {
        self.unwind_indent(-1);
        self.remove_possible_simple_key()?;
        self.allow_simple_key = false;

        let start_mark = self.get_mark();
        self.forward(3);
        let end_mark = self.get_mark();
        self.tokens
            .push_back(Token::new(kind, start_mark, end_mark));
        Ok(())
    }

    fn fetch_flow_collection_start(&mut self, kind: TokenKind) -> Result<()> {
        self.save_possible_simple_key()?;
        self.flow_level += 1;
        self.allow_simple_key = true;

        let start_mark = self.get_mark();
        self.forward1();
        let end_mark = self.get_mark();
        self.tokens
            .push_back(Token::new(kind, start_mark, end_mark));
        Ok(())
    }

    fn fetch_flow_collection_end(&mut self, kind: TokenKind) -> Result<()> {
        self.remove_possible_simple_key()?;
        self.flow_level = self.flow_level.saturating_sub(1);
        self.allow_simple_key = false;

        let start_mark = self.get_mark();
        self.forward1();
        let end_mark = self.get_mark();
        self.tokens
            .push_back(Token::new(kind, start_mark, end_mark));
        Ok(())
    }

    fn fetch_flow_entry(&mut self) -> Result<()> {
        self.allow_simple_key = true;
        self.remove_possible_simple_key()?;

        let start_mark = self.get_mark();
        self.forward1();
        let end_mark = self.get_mark();
        self.tokens
            .push_back(Token::new(TokenKind::FlowEntry, start_mark, end_mark));
        Ok(())
    }

    fn fetch_block_entry(&mut self) -> Result<()> {
        if self.flow_level == 0 {
            if !self.allow_simple_key {
                self.failed = true;
                return Err(self.error(None, None, "sequence entries are not allowed here".into()));
            }
            if self.add_indent(self.column as i64) {
                let mark = self.get_mark();
                self.tokens
                    .push_back(Token::new(TokenKind::BlockSequenceStart, mark, mark));
            }
        }

        self.allow_simple_key = true;
        self.remove_possible_simple_key()?;

        let start_mark = self.get_mark();
        self.forward1();
        let end_mark = self.get_mark();
        self.tokens
            .push_back(Token::new(TokenKind::BlockEntry, start_mark, end_mark));
        Ok(())
    }

    fn fetch_key(&mut self) -> Result<()> {
        if self.flow_level == 0 {
            if !self.allow_simple_key {
                self.failed = true;
                return Err(self.error(None, None, "mapping keys are not allowed here".into()));
            }
            if self.add_indent(self.column as i64) {
                let mark = self.get_mark();
                self.tokens
                    .push_back(Token::new(TokenKind::BlockMappingStart, mark, mark));
            }
        }

        self.allow_simple_key = self.flow_level == 0;
        self.remove_possible_simple_key()?;

        let start_mark = self.get_mark();
        self.forward1();
        let end_mark = self.get_mark();
        self.tokens
            .push_back(Token::new(TokenKind::Key, start_mark, end_mark));
        Ok(())
    }

    fn fetch_value(&mut self) -> Result<()> {
        if let Some(key) = self.possible_simple_keys.remove(&self.flow_level) {
            // Add KEY.
            let insert_at = key.token_number - self.tokens_taken;
            self.tokens
                .insert(insert_at, Token::new(TokenKind::Key, key.mark, key.mark));

            // If this key starts a new block mapping, we need to add
            // BLOCK-MAPPING-START.
            if self.flow_level == 0 && self.add_indent(key.mark.column as i64) {
                self.tokens.insert(
                    insert_at,
                    Token::new(TokenKind::BlockMappingStart, key.mark, key.mark),
                );
            }

            self.allow_simple_key = false;
        } else {
            if self.flow_level == 0 {
                if !self.allow_simple_key {
                    self.failed = true;
                    return Err(self.error(
                        None,
                        None,
                        "mapping values are not allowed here".into(),
                    ));
                }
                if self.add_indent(self.column as i64) {
                    let mark = self.get_mark();
                    self.tokens
                        .push_back(Token::new(TokenKind::BlockMappingStart, mark, mark));
                }
            }

            self.allow_simple_key = self.flow_level == 0;
            self.remove_possible_simple_key()?;
        }

        let start_mark = self.get_mark();
        self.forward1();
        let end_mark = self.get_mark();
        self.tokens
            .push_back(Token::new(TokenKind::Value, start_mark, end_mark));
        Ok(())
    }

    fn fetch_anchor(&mut self, is_alias: bool) -> Result<()> {
        self.save_possible_simple_key()?;
        self.allow_simple_key = false;
        let token = self.scan_anchor(is_alias)?;
        self.tokens.push_back(token);
        Ok(())
    }

    fn fetch_tag(&mut self) -> Result<()> {
        self.save_possible_simple_key()?;
        self.allow_simple_key = false;
        let token = self.scan_tag()?;
        self.tokens.push_back(token);
        Ok(())
    }

    fn fetch_block_scalar(&mut self, style: char) -> Result<()> {
        self.allow_simple_key = true;
        self.remove_possible_simple_key()?;
        let token = self.scan_block_scalar(style)?;
        self.tokens.push_back(token);
        Ok(())
    }

    fn fetch_flow_scalar(&mut self, style: char) -> Result<()> {
        self.save_possible_simple_key()?;
        self.allow_simple_key = false;
        let token = self.scan_flow_scalar(style)?;
        self.tokens.push_back(token);
        Ok(())
    }

    fn fetch_plain(&mut self) -> Result<()> {
        self.save_possible_simple_key()?;
        self.allow_simple_key = false;
        let token = self.scan_plain();
        self.tokens.push_back(token);
        Ok(())
    }

    // --- Checkers ----------------------------------------------------------

    fn check_directive(&self) -> bool {
        // DIRECTIVE: ^ '%' ...
        self.column == 0
    }

    fn check_document_start(&self) -> bool {
        // DOCUMENT-START: ^ '---' (' '|'\n')
        self.column == 0 && self.prefix_is("---") && in_z_blank_break(self.peek(3))
    }

    fn check_document_end(&self) -> bool {
        // DOCUMENT-END: ^ '...' (' '|'\n')
        self.column == 0 && self.prefix_is("...") && in_z_blank_break(self.peek(3))
    }

    fn check_block_entry(&self) -> bool {
        // BLOCK-ENTRY: '-' (' '|'\n')
        in_z_blank_break(self.peek(1))
    }

    fn check_key(&self) -> bool {
        // KEY(flow context): '?'
        if self.flow_level != 0 {
            true
        } else {
            // KEY(block context): '?' (' '|'\n')
            in_z_blank_break(self.peek(1))
        }
    }

    fn check_value(&self) -> bool {
        if self.flow_level != 0 {
            true
        } else {
            in_z_blank_break(self.peek(1))
        }
    }

    fn check_plain(&self) -> bool {
        let ch = self.peek0();
        let not_special = !in_z_blank_break(ch)
            && !matches!(
                ch,
                '-' | '?'
                    | ':'
                    | ','
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | '#'
                    | '&'
                    | '*'
                    | '!'
                    | '|'
                    | '>'
                    | '\''
                    | '"'
                    | '%'
                    | '@'
                    | '`'
            );
        not_special
            || (!in_z_blank_break(self.peek(1))
                && (ch == '-' || (self.flow_level == 0 && (ch == '?' || ch == ':'))))
    }

    // --- Scanners ----------------------------------------------------------

    fn scan_to_next_token(&mut self) {
        if self.pointer == 0 && self.peek0() == '\u{FEFF}' {
            self.forward1();
        }
        let mut found = false;
        while !found {
            while self.peek0() == ' ' {
                self.forward1();
            }
            if self.peek0() == '#' {
                while !in_z_break(self.peek0()) {
                    self.forward1();
                }
            }
            if !self.scan_line_break().is_empty() {
                if self.flow_level == 0 {
                    self.allow_simple_key = true;
                }
            } else {
                found = true;
            }
        }
    }

    fn scan_directive(&mut self) -> Result<Token> {
        let start_mark = self.get_mark();
        self.forward1();
        let name = self.scan_directive_name(start_mark)?;
        let value;
        let end_mark;
        if name == "YAML" {
            let (major, minor) = self.scan_yaml_directive_value(start_mark)?;
            value = DirectiveValue::Yaml(major, minor);
            end_mark = self.get_mark();
        } else if name == "TAG" {
            let (handle, prefix) = self.scan_tag_directive_value(start_mark)?;
            value = DirectiveValue::Tag(handle, prefix);
            end_mark = self.get_mark();
        } else {
            value = DirectiveValue::Other;
            end_mark = self.get_mark();
            while !in_z_break(self.peek0()) {
                self.forward1();
            }
        }
        self.scan_directive_ignored_line(start_mark)?;
        Ok(Token::new(
            TokenKind::Directive { name, value },
            start_mark,
            end_mark,
        ))
    }

    fn scan_directive_name(&mut self, start_mark: Mark) -> Result<String> {
        let mut length = 0;
        let mut ch = self.peek(length);
        while is_alnum(ch) {
            length += 1;
            ch = self.peek(length);
        }
        if length == 0 {
            return Err(YamlError::new(
                Some("while scanning a directive"),
                Some(start_mark),
                format!(
                    "expected alphabetic or numeric character, but found {}",
                    py_repr_char(ch)
                ),
                Some(self.get_mark()),
            ));
        }
        let value = self.prefix(length);
        self.forward(length);
        let ch = self.peek0();
        if !in_z_space_break(ch) {
            return Err(YamlError::new(
                Some("while scanning a directive"),
                Some(start_mark),
                format!(
                    "expected alphabetic or numeric character, but found {}",
                    py_repr_char(ch)
                ),
                Some(self.get_mark()),
            ));
        }
        Ok(value)
    }

    fn scan_yaml_directive_value(&mut self, start_mark: Mark) -> Result<(u64, u64)> {
        while self.peek0() == ' ' {
            self.forward1();
        }
        let major = self.scan_yaml_directive_number(start_mark)?;
        if self.peek0() != '.' {
            return Err(YamlError::new(
                Some("while scanning a directive"),
                Some(start_mark),
                format!(
                    "expected a digit or '.', but found {}",
                    py_repr_char(self.peek0())
                ),
                Some(self.get_mark()),
            ));
        }
        self.forward1();
        let minor = self.scan_yaml_directive_number(start_mark)?;
        if !in_z_space_break(self.peek0()) {
            return Err(YamlError::new(
                Some("while scanning a directive"),
                Some(start_mark),
                format!(
                    "expected a digit or ' ', but found {}",
                    py_repr_char(self.peek0())
                ),
                Some(self.get_mark()),
            ));
        }
        Ok((major, minor))
    }

    fn scan_yaml_directive_number(&mut self, start_mark: Mark) -> Result<u64> {
        let ch = self.peek0();
        if !ch.is_ascii_digit() {
            return Err(YamlError::new(
                Some("while scanning a directive"),
                Some(start_mark),
                format!("expected a digit, but found {}", py_repr_char(ch)),
                Some(self.get_mark()),
            ));
        }
        let mut length = 0;
        while self.peek(length).is_ascii_digit() {
            length += 1;
        }
        let value = self.prefix(length).parse::<u64>().unwrap_or(u64::MAX);
        self.forward(length);
        Ok(value)
    }

    fn scan_tag_directive_value(&mut self, start_mark: Mark) -> Result<(String, String)> {
        while self.peek0() == ' ' {
            self.forward1();
        }
        let handle = self.scan_tag_directive_handle(start_mark)?;
        while self.peek0() == ' ' {
            self.forward1();
        }
        let prefix = self.scan_tag_directive_prefix(start_mark)?;
        Ok((handle, prefix))
    }

    fn scan_tag_directive_handle(&mut self, start_mark: Mark) -> Result<String> {
        let value = self.scan_tag_handle("directive", start_mark)?;
        let ch = self.peek0();
        if ch != ' ' {
            return Err(YamlError::new(
                Some("while scanning a directive"),
                Some(start_mark),
                format!("expected ' ', but found {}", py_repr_char(ch)),
                Some(self.get_mark()),
            ));
        }
        Ok(value)
    }

    fn scan_tag_directive_prefix(&mut self, start_mark: Mark) -> Result<String> {
        let value = self.scan_tag_uri("directive", start_mark)?;
        let ch = self.peek0();
        if !in_z_space_break(ch) {
            return Err(YamlError::new(
                Some("while scanning a directive"),
                Some(start_mark),
                format!("expected ' ', but found {}", py_repr_char(ch)),
                Some(self.get_mark()),
            ));
        }
        Ok(value)
    }

    fn scan_directive_ignored_line(&mut self, start_mark: Mark) -> Result<()> {
        while self.peek0() == ' ' {
            self.forward1();
        }
        if self.peek0() == '#' {
            while !in_z_break(self.peek0()) {
                self.forward1();
            }
        }
        let ch = self.peek0();
        if !in_z_break(ch) {
            return Err(YamlError::new(
                Some("while scanning a directive"),
                Some(start_mark),
                format!(
                    "expected a comment or a line break, but found {}",
                    py_repr_char(ch)
                ),
                Some(self.get_mark()),
            ));
        }
        self.scan_line_break();
        Ok(())
    }

    fn scan_anchor(&mut self, is_alias: bool) -> Result<Token> {
        let start_mark = self.get_mark();
        let name = if is_alias { "alias" } else { "anchor" };
        self.forward1();
        let mut length = 0;
        let mut ch = self.peek(length);
        while is_alnum(ch) {
            length += 1;
            ch = self.peek(length);
        }
        if length == 0 {
            return Err(YamlError::new(
                Some(&format!("while scanning an {name}")),
                Some(start_mark),
                format!(
                    "expected alphabetic or numeric character, but found {}",
                    py_repr_char(ch)
                ),
                Some(self.get_mark()),
            ));
        }
        let value = self.prefix(length);
        self.forward(length);
        let ch = self.peek0();
        if !in_z_blank_break(ch) && !matches!(ch, '?' | ':' | ',' | ']' | '}' | '%' | '@' | '`') {
            return Err(YamlError::new(
                Some(&format!("while scanning an {name}")),
                Some(start_mark),
                format!(
                    "expected alphabetic or numeric character, but found {}",
                    py_repr_char(ch)
                ),
                Some(self.get_mark()),
            ));
        }
        let end_mark = self.get_mark();
        let kind = if is_alias {
            TokenKind::Alias(value)
        } else {
            TokenKind::Anchor(value)
        };
        Ok(Token::new(kind, start_mark, end_mark))
    }

    fn scan_tag(&mut self) -> Result<Token> {
        let start_mark = self.get_mark();
        let mut ch = self.peek(1);
        let handle: Option<String>;
        let suffix: String;
        if ch == '<' {
            handle = None;
            self.forward(2);
            suffix = self.scan_tag_uri("tag", start_mark)?;
            if self.peek0() != '>' {
                return Err(YamlError::new(
                    Some("while parsing a tag"),
                    Some(start_mark),
                    format!("expected '>', but found {}", py_repr_char(self.peek0())),
                    Some(self.get_mark()),
                ));
            }
            self.forward1();
        } else if in_z_blank_break(ch) {
            handle = None;
            suffix = "!".to_string();
            self.forward1();
        } else {
            let mut length = 1;
            let mut use_handle = false;
            while !in_z_space_break(ch) {
                if ch == '!' {
                    use_handle = true;
                    break;
                }
                length += 1;
                ch = self.peek(length);
            }
            let _ = length;
            if use_handle {
                handle = Some(self.scan_tag_handle("tag", start_mark)?);
            } else {
                handle = Some("!".to_string());
                self.forward1();
            }
            suffix = self.scan_tag_uri("tag", start_mark)?;
        }
        let ch = self.peek0();
        if !in_z_space_break(ch) {
            return Err(YamlError::new(
                Some("while scanning a tag"),
                Some(start_mark),
                format!("expected ' ', but found {}", py_repr_char(ch)),
                Some(self.get_mark()),
            ));
        }
        let end_mark = self.get_mark();
        Ok(Token::new(
            TokenKind::Tag { handle, suffix },
            start_mark,
            end_mark,
        ))
    }

    fn scan_block_scalar(&mut self, style: char) -> Result<Token> {
        let folded = style == '>';

        let mut chunks = String::new();
        let start_mark = self.get_mark();

        // Scan the header.
        self.forward1();
        let (chomping, increment) = self.scan_block_scalar_indicators(start_mark)?;
        self.scan_block_scalar_ignored_line(start_mark)?;

        // Determine the indentation level and go to the first non-empty line.
        let mut min_indent = self.indent + 1;
        if min_indent < 1 {
            min_indent = 1;
        }
        let mut breaks: Vec<String>;
        let mut end_mark: Mark;
        let indent: i64;
        if let Some(increment) = increment {
            indent = min_indent + increment as i64 - 1;
            let (b, e) = self.scan_block_scalar_breaks(indent);
            breaks = b;
            end_mark = e;
        } else {
            let (b, max_indent, e) = self.scan_block_scalar_indentation();
            breaks = b;
            end_mark = e;
            indent = min_indent.max(max_indent);
        }
        let mut line_break = String::new();

        // Scan the inner part of the block scalar.
        while self.column as i64 == indent && self.peek0() != Z {
            for b in &breaks {
                chunks.push_str(b);
            }
            let leading_non_space = !matches!(self.peek0(), ' ' | '\t');
            let mut length = 0;
            while !in_z_break(self.peek(length)) {
                length += 1;
            }
            chunks.push_str(&self.prefix(length));
            self.forward(length);
            line_break = self.scan_line_break();
            let (b, e) = self.scan_block_scalar_breaks(indent);
            breaks = b;
            end_mark = e;
            if self.column as i64 == indent && self.peek0() != Z {
                // Folding rules per the specification.
                if folded
                    && line_break == "\n"
                    && leading_non_space
                    && !matches!(self.peek0(), ' ' | '\t')
                {
                    if breaks.is_empty() {
                        chunks.push(' ');
                    }
                } else {
                    chunks.push_str(&line_break);
                }
            } else {
                break;
            }
        }

        // Chomp the tail.
        if chomping != Some(false) {
            chunks.push_str(&line_break);
        }
        if chomping == Some(true) {
            for b in &breaks {
                chunks.push_str(b);
            }
        }

        Ok(Token::new(
            TokenKind::Scalar {
                value: chunks,
                plain: false,
                style: Some(style),
            },
            start_mark,
            end_mark,
        ))
    }

    fn scan_block_scalar_indicators(
        &mut self,
        start_mark: Mark,
    ) -> Result<(Option<bool>, Option<u32>)> {
        let mut chomping: Option<bool> = None;
        let mut increment: Option<u32> = None;
        let mut ch = self.peek0();
        if ch == '+' || ch == '-' {
            chomping = Some(ch == '+');
            self.forward1();
            ch = self.peek0();
            if ch.is_ascii_digit() {
                let inc = ch.to_digit(10).unwrap();
                if inc == 0 {
                    return Err(YamlError::new(
                        Some("while scanning a block scalar"),
                        Some(start_mark),
                        "expected indentation indicator in the range 1-9, but found 0".into(),
                        Some(self.get_mark()),
                    ));
                }
                increment = Some(inc);
                self.forward1();
            }
        } else if ch.is_ascii_digit() {
            let inc = ch.to_digit(10).unwrap();
            if inc == 0 {
                return Err(YamlError::new(
                    Some("while scanning a block scalar"),
                    Some(start_mark),
                    "expected indentation indicator in the range 1-9, but found 0".into(),
                    Some(self.get_mark()),
                ));
            }
            increment = Some(inc);
            self.forward1();
            ch = self.peek0();
            if ch == '+' || ch == '-' {
                chomping = Some(ch == '+');
                self.forward1();
            }
        }
        let ch = self.peek0();
        if !in_z_space_break(ch) {
            return Err(YamlError::new(
                Some("while scanning a block scalar"),
                Some(start_mark),
                format!(
                    "expected chomping or indentation indicators, but found {}",
                    py_repr_char(ch)
                ),
                Some(self.get_mark()),
            ));
        }
        Ok((chomping, increment))
    }

    fn scan_block_scalar_ignored_line(&mut self, start_mark: Mark) -> Result<()> {
        while self.peek0() == ' ' {
            self.forward1();
        }
        if self.peek0() == '#' {
            while !in_z_break(self.peek0()) {
                self.forward1();
            }
        }
        let ch = self.peek0();
        if !in_z_break(ch) {
            return Err(YamlError::new(
                Some("while scanning a block scalar"),
                Some(start_mark),
                format!(
                    "expected a comment or a line break, but found {}",
                    py_repr_char(ch)
                ),
                Some(self.get_mark()),
            ));
        }
        self.scan_line_break();
        Ok(())
    }

    fn scan_block_scalar_indentation(&mut self) -> (Vec<String>, i64, Mark) {
        let mut chunks = Vec::new();
        let mut max_indent: i64 = 0;
        let mut end_mark = self.get_mark();
        while matches!(
            self.peek0(),
            ' ' | '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}'
        ) {
            if self.peek0() != ' ' {
                chunks.push(self.scan_line_break());
                end_mark = self.get_mark();
            } else {
                self.forward1();
                if self.column as i64 > max_indent {
                    max_indent = self.column as i64;
                }
            }
        }
        (chunks, max_indent, end_mark)
    }

    fn scan_block_scalar_breaks(&mut self, indent: i64) -> (Vec<String>, Mark) {
        let mut chunks = Vec::new();
        let mut end_mark = self.get_mark();
        while (self.column as i64) < indent && self.peek0() == ' ' {
            self.forward1();
        }
        while matches!(
            self.peek0(),
            '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}'
        ) {
            chunks.push(self.scan_line_break());
            end_mark = self.get_mark();
            while (self.column as i64) < indent && self.peek0() == ' ' {
                self.forward1();
            }
        }
        (chunks, end_mark)
    }

    fn scan_flow_scalar(&mut self, style: char) -> Result<Token> {
        let double = style == '"';
        let mut chunks = String::new();
        let start_mark = self.get_mark();
        let quote = self.peek0();
        self.forward1();
        self.scan_flow_scalar_non_spaces(double, start_mark, &mut chunks)?;
        while self.peek0() != quote {
            self.scan_flow_scalar_spaces(start_mark, &mut chunks)?;
            self.scan_flow_scalar_non_spaces(double, start_mark, &mut chunks)?;
        }
        self.forward1();
        let end_mark = self.get_mark();
        Ok(Token::new(
            TokenKind::Scalar {
                value: chunks,
                plain: false,
                style: Some(style),
            },
            start_mark,
            end_mark,
        ))
    }

    fn escape_replacement(ch: char) -> Option<char> {
        Some(match ch {
            '0' => '\0',
            'a' => '\x07',
            'b' => '\x08',
            't' | '\t' => '\x09',
            'n' => '\x0A',
            'v' => '\x0B',
            'f' => '\x0C',
            'r' => '\x0D',
            'e' => '\x1B',
            ' ' => '\x20',
            '"' => '"',
            '\\' => '\\',
            '/' => '/',
            'N' => '\u{85}',
            '_' => '\u{A0}',
            'L' => '\u{2028}',
            'P' => '\u{2029}',
            _ => return None,
        })
    }

    fn escape_code_length(ch: char) -> Option<usize> {
        match ch {
            'x' => Some(2),
            'u' => Some(4),
            'U' => Some(8),
            _ => None,
        }
    }

    fn scan_flow_scalar_non_spaces(
        &mut self,
        double: bool,
        start_mark: Mark,
        chunks: &mut String,
    ) -> Result<()> {
        loop {
            let mut length = 0;
            while !matches!(
                self.peek(length),
                '\'' | '"'
                    | '\\'
                    | '\0'
                    | ' '
                    | '\t'
                    | '\r'
                    | '\n'
                    | '\u{85}'
                    | '\u{2028}'
                    | '\u{2029}'
            ) {
                length += 1;
            }
            if length > 0 {
                chunks.push_str(&self.prefix(length));
                self.forward(length);
            }
            let ch = self.peek0();
            if !double && ch == '\'' && self.peek(1) == '\'' {
                chunks.push('\'');
                self.forward(2);
            } else if (double && ch == '\'') || (!double && (ch == '"' || ch == '\\')) {
                chunks.push(ch);
                self.forward1();
            } else if double && ch == '\\' {
                self.forward1();
                let ch = self.peek0();
                if let Some(replacement) = Self::escape_replacement(ch) {
                    chunks.push(replacement);
                    self.forward1();
                } else if let Some(length) = Self::escape_code_length(ch) {
                    self.forward1();
                    for k in 0..length {
                        if !self.peek(k).is_ascii_hexdigit() {
                            return Err(YamlError::new(
                                Some("while scanning a double-quoted scalar"),
                                Some(start_mark),
                                format!(
                                    "expected escape sequence of {} hexadecimal numbers, but found {}",
                                    length,
                                    py_repr_char(self.peek(k))
                                ),
                                Some(self.get_mark()),
                            ));
                        }
                    }
                    let code = u32::from_str_radix(&self.prefix(length), 16).unwrap_or(0);
                    chunks.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                    self.forward(length);
                } else if matches!(ch, '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}') {
                    self.scan_line_break();
                    self.scan_flow_scalar_breaks(start_mark, chunks)?;
                } else {
                    return Err(YamlError::new(
                        Some("while scanning a double-quoted scalar"),
                        Some(start_mark),
                        format!("found unknown escape character {}", py_repr_char(ch)),
                        Some(self.get_mark()),
                    ));
                }
            } else {
                return Ok(());
            }
        }
    }

    fn scan_flow_scalar_spaces(&mut self, start_mark: Mark, chunks: &mut String) -> Result<()> {
        let mut length = 0;
        while matches!(self.peek(length), ' ' | '\t') {
            length += 1;
        }
        let whitespaces = self.prefix(length);
        self.forward(length);
        let ch = self.peek0();
        if ch == Z {
            return Err(YamlError::new(
                Some("while scanning a quoted scalar"),
                Some(start_mark),
                "found unexpected end of stream".into(),
                Some(self.get_mark()),
            ));
        } else if matches!(ch, '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}') {
            let line_break = self.scan_line_break();
            let mut breaks = String::new();
            let had_breaks = self.scan_flow_scalar_breaks_into(start_mark, &mut breaks)?;
            if line_break != "\n" {
                chunks.push_str(&line_break);
            } else if !had_breaks {
                chunks.push(' ');
            }
            chunks.push_str(&breaks);
        } else {
            chunks.push_str(&whitespaces);
        }
        Ok(())
    }

    fn scan_flow_scalar_breaks(&mut self, start_mark: Mark, chunks: &mut String) -> Result<()> {
        let mut breaks = String::new();
        self.scan_flow_scalar_breaks_into(start_mark, &mut breaks)?;
        chunks.push_str(&breaks);
        Ok(())
    }

    /// Returns whether any break was consumed.
    fn scan_flow_scalar_breaks_into(
        &mut self,
        start_mark: Mark,
        chunks: &mut String,
    ) -> Result<bool> {
        let mut any = false;
        loop {
            // Instead of checking indentation, we check for document
            // separators.
            if (self.prefix_is("---") || self.prefix_is("...")) && in_z_blank_break(self.peek(3)) {
                return Err(YamlError::new(
                    Some("while scanning a quoted scalar"),
                    Some(start_mark),
                    "found unexpected document separator".into(),
                    Some(self.get_mark()),
                ));
            }
            while matches!(self.peek0(), ' ' | '\t') {
                self.forward1();
            }
            if matches!(
                self.peek0(),
                '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}'
            ) {
                chunks.push_str(&self.scan_line_break());
                any = true;
            } else {
                return Ok(any);
            }
        }
    }

    fn scan_plain(&mut self) -> Token {
        let mut chunks = String::new();
        let start_mark = self.get_mark();
        let mut end_mark = start_mark;
        let indent = self.indent + 1;
        let mut spaces = String::new();
        loop {
            let mut length = 0;
            if self.peek0() == '#' {
                break;
            }
            loop {
                let ch = self.peek(length);
                if in_z_blank_break(ch)
                    || (ch == ':'
                        && (in_z_blank_break(self.peek(length + 1))
                            || (self.flow_level != 0
                                && matches!(self.peek(length + 1), ',' | '[' | ']' | '{' | '}'))))
                    || (self.flow_level != 0 && matches!(ch, ',' | '?' | '[' | ']' | '{' | '}'))
                {
                    break;
                }
                length += 1;
            }
            if length == 0 {
                break;
            }
            self.allow_simple_key = false;
            chunks.push_str(&spaces);
            chunks.push_str(&self.prefix(length));
            self.forward(length);
            end_mark = self.get_mark();
            spaces = self.scan_plain_spaces();
            if spaces.is_empty()
                || self.peek0() == '#'
                || (self.flow_level == 0 && (self.column as i64) < indent)
            {
                break;
            }
        }
        Token::new(
            TokenKind::Scalar {
                value: chunks,
                plain: true,
                style: None,
            },
            start_mark,
            end_mark,
        )
    }

    fn scan_plain_spaces(&mut self) -> String {
        let mut chunks = String::new();
        let mut length = 0;
        while self.peek(length) == ' ' {
            length += 1;
        }
        let whitespaces = self.prefix(length);
        self.forward(length);
        let ch = self.peek0();
        if matches!(ch, '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}') {
            let line_break = self.scan_line_break();
            self.allow_simple_key = true;
            if (self.prefix_is("---") || self.prefix_is("...")) && in_z_blank_break(self.peek(3)) {
                return String::new();
            }
            let mut breaks = String::new();
            while matches!(
                self.peek0(),
                ' ' | '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}'
            ) {
                if self.peek0() == ' ' {
                    self.forward1();
                } else {
                    breaks.push_str(&self.scan_line_break());
                    if (self.prefix_is("---") || self.prefix_is("..."))
                        && in_z_blank_break(self.peek(3))
                    {
                        return String::new();
                    }
                }
            }
            if line_break != "\n" {
                chunks.push_str(&line_break);
            } else if breaks.is_empty() {
                chunks.push(' ');
            }
            chunks.push_str(&breaks);
        } else if !whitespaces.is_empty() {
            chunks.push_str(&whitespaces);
        }
        chunks
    }

    fn scan_tag_handle(&mut self, name: &str, start_mark: Mark) -> Result<String> {
        let ch = self.peek0();
        if ch != '!' {
            return Err(YamlError::new(
                Some(&format!("while scanning a {name}")),
                Some(start_mark),
                format!("expected '!', but found {}", py_repr_char(ch)),
                Some(self.get_mark()),
            ));
        }
        let mut length = 1;
        let mut ch = self.peek(length);
        if ch != ' ' {
            while is_alnum(ch) {
                length += 1;
                ch = self.peek(length);
            }
            if ch != '!' {
                self.forward(length);
                return Err(YamlError::new(
                    Some(&format!("while scanning a {name}")),
                    Some(start_mark),
                    format!("expected '!', but found {}", py_repr_char(ch)),
                    Some(self.get_mark()),
                ));
            }
            length += 1;
        }
        let value = self.prefix(length);
        self.forward(length);
        Ok(value)
    }

    fn scan_tag_uri(&mut self, name: &str, start_mark: Mark) -> Result<String> {
        let mut chunks = String::new();
        let mut length = 0;
        let mut ch = self.peek(length);
        while ch.is_ascii_alphanumeric()
            || matches!(
                ch,
                '-' | ';'
                    | '/'
                    | '?'
                    | ':'
                    | '@'
                    | '&'
                    | '='
                    | '+'
                    | '$'
                    | ','
                    | '_'
                    | '.'
                    | '!'
                    | '~'
                    | '*'
                    | '\''
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '%'
            )
        {
            if ch == '%' {
                chunks.push_str(&self.prefix(length));
                self.forward(length);
                length = 0;
                chunks.push_str(&self.scan_uri_escapes(name, start_mark)?);
            } else {
                length += 1;
            }
            ch = self.peek(length);
        }
        if length > 0 {
            chunks.push_str(&self.prefix(length));
            self.forward(length);
        }
        if chunks.is_empty() {
            return Err(YamlError::new(
                Some(&format!("while parsing a {name}")),
                Some(start_mark),
                format!("expected URI, but found {}", py_repr_char(ch)),
                Some(self.get_mark()),
            ));
        }
        Ok(chunks)
    }

    fn scan_uri_escapes(&mut self, name: &str, start_mark: Mark) -> Result<String> {
        let mut codes: Vec<u8> = Vec::new();
        let mark = self.get_mark();
        while self.peek0() == '%' {
            self.forward1();
            for k in 0..2 {
                if !self.peek(k).is_ascii_hexdigit() {
                    return Err(YamlError::new(
                        Some(&format!("while scanning a {name}")),
                        Some(start_mark),
                        format!(
                            "expected URI escape sequence of 2 hexadecimal numbers, but found {}",
                            py_repr_char(self.peek(k))
                        ),
                        Some(self.get_mark()),
                    ));
                }
            }
            codes.push(u8::from_str_radix(&self.prefix(2), 16).unwrap_or(0));
            self.forward(2);
        }
        match String::from_utf8(codes) {
            Ok(value) => Ok(value),
            Err(e) => Err(YamlError::new(
                Some(&format!("while scanning a {name}")),
                Some(start_mark),
                e.to_string(),
                Some(mark),
            )),
        }
    }

    fn scan_line_break(&mut self) -> String {
        let ch = self.peek0();
        if matches!(ch, '\r' | '\n' | '\u{85}') {
            if self.prefix_is("\r\n") {
                self.forward(2);
            } else {
                self.forward1();
            }
            "\n".to_string()
        } else if matches!(ch, '\u{2028}' | '\u{2029}') {
            self.forward1();
            ch.to_string()
        } else {
            String::new()
        }
    }
}
