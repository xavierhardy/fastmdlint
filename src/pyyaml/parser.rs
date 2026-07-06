//! Event parser for YAML token streams. It reports the first syntax error
//! with PyYAML-compatible messages and positions, and produces the event
//! stream used to load YAML documents into values.

use std::collections::HashMap;

use super::error::{Mark, YamlError};
use super::pyrepr::py_repr;
use super::scanner::Scanner;
use super::tokens::{DirectiveValue, Token, TokenKind};

type Result<T> = std::result::Result<T, YamlError>;

/// Parsing events, carrying just enough payload to compose values.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    StreamStart,
    StreamEnd,
    DocumentStart,
    DocumentEnd,
    Alias {
        value: String,
        mark: Mark,
    },
    Scalar {
        value: String,
        plain: bool,
        style: Option<char>,
        anchor: Option<String>,
        tag: Option<String>,
    },
    SequenceStart {
        anchor: Option<String>,
        tag: Option<String>,
    },
    SequenceEnd,
    MappingStart {
        anchor: Option<String>,
        tag: Option<String>,
    },
    MappingEnd,
}

fn empty_scalar() -> Event {
    Event::Scalar {
        value: String::new(),
        plain: true,
        style: None,
        anchor: None,
        tag: None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    StreamStart,
    ImplicitDocumentStart,
    DocumentStart,
    DocumentContent,
    DocumentEnd,
    BlockNode,
    BlockNodeOrIndentlessSequence,
    FlowNode,
    BlockSequenceFirstEntry,
    BlockSequenceEntry,
    IndentlessSequenceEntry,
    BlockMappingFirstKey,
    BlockMappingKey,
    BlockMappingValue,
    FlowSequenceFirstEntry,
    FlowSequenceEntry { first: bool },
    FlowSequenceEntryMappingKey,
    FlowSequenceEntryMappingValue,
    FlowSequenceEntryMappingEnd,
    FlowMappingFirstKey,
    FlowMappingKey { first: bool },
    FlowMappingValue,
    FlowMappingEmptyValue,
    End,
}

fn kind_matches(token: &Token, choice: Choice) -> bool {
    use TokenKind::*;
    match choice {
        Choice::Directive => matches!(token.kind, Directive { .. }),
        Choice::DocumentStart => matches!(token.kind, DocumentStart),
        Choice::DocumentEnd => matches!(token.kind, DocumentEnd),
        Choice::StreamEnd => matches!(token.kind, StreamEnd),
        Choice::BlockEntry => matches!(token.kind, BlockEntry),
        Choice::BlockEnd => matches!(token.kind, BlockEnd),
        Choice::Key => matches!(token.kind, Key),
        Choice::Value => matches!(token.kind, Value),
        Choice::FlowEntry => matches!(token.kind, FlowEntry),
        Choice::FlowSequenceStart => matches!(token.kind, FlowSequenceStart),
        Choice::FlowSequenceEnd => matches!(token.kind, FlowSequenceEnd),
        Choice::FlowMappingStart => matches!(token.kind, FlowMappingStart),
        Choice::FlowMappingEnd => matches!(token.kind, FlowMappingEnd),
        Choice::BlockSequenceStart => matches!(token.kind, BlockSequenceStart),
        Choice::BlockMappingStart => matches!(token.kind, BlockMappingStart),
        Choice::Alias => matches!(token.kind, Alias(_)),
        Choice::Anchor => matches!(token.kind, Anchor(_)),
        Choice::Tag => matches!(token.kind, Tag { .. }),
        Choice::Scalar => matches!(token.kind, Scalar { .. }),
    }
}

#[derive(Debug, Clone, Copy)]
enum Choice {
    Directive,
    DocumentStart,
    DocumentEnd,
    StreamEnd,
    BlockEntry,
    BlockEnd,
    Key,
    Value,
    FlowEntry,
    FlowSequenceStart,
    FlowSequenceEnd,
    FlowMappingStart,
    FlowMappingEnd,
    BlockSequenceStart,
    BlockMappingStart,
    Alias,
    Anchor,
    Tag,
    Scalar,
}

pub struct Parser {
    scanner: Scanner,
    lookahead: Option<Token>,
    state: State,
    states: Vec<State>,
    marks: Vec<Mark>,
    yaml_version: Option<(u64, u64)>,
    tag_handles: HashMap<String, String>,
}

impl Parser {
    pub fn new(content: &str) -> Self {
        Self {
            scanner: Scanner::new(content),
            lookahead: None,
            state: State::StreamStart,
            states: Vec::new(),
            marks: Vec::new(),
            yaml_version: None,
            tag_handles: HashMap::new(),
        }
    }

    /// Drive the parser over the whole stream; return the first error if
    /// any.
    pub fn check_syntax(content: &str) -> Option<YamlError> {
        let mut parser = Parser::new(content);
        loop {
            match parser.next_event() {
                Ok(Some(_)) => continue,
                Ok(None) => return None,
                Err(e) => return Some(e),
            }
        }
    }

    /// Produce the next event, or `None` at the end of the stream.
    pub fn next_event(&mut self) -> Result<Option<Event>> {
        loop {
            if self.state == State::End {
                return Ok(None);
            }
            if let Some(event) = self.step()? {
                return Ok(Some(event));
            }
        }
    }

    fn peek_token(&mut self) -> Result<Option<&Token>> {
        if self.lookahead.is_none() {
            self.lookahead = self.scanner.get_token()?;
        }
        Ok(self.lookahead.as_ref())
    }

    fn get_token(&mut self) -> Result<Option<Token>> {
        if self.lookahead.is_none() {
            self.lookahead = self.scanner.get_token()?;
        }
        Ok(self.lookahead.take())
    }

    fn check_token(&mut self, choices: &[Choice]) -> Result<bool> {
        match self.peek_token()? {
            Some(token) => Ok(choices.iter().any(|&c| kind_matches(token, c))),
            None => Ok(false),
        }
    }

    fn peek_required(&mut self) -> Result<Token> {
        match self.peek_token()? {
            Some(t) => Ok(t.clone()),
            None => Ok(Token::new(
                TokenKind::StreamEnd,
                Mark::default(),
                Mark::default(),
            )),
        }
    }

    /// Run one state transition, possibly producing an event.
    fn step(&mut self) -> Result<Option<Event>> {
        match self.state {
            State::End => Ok(None),
            State::StreamStart => {
                self.get_token()?;
                self.state = State::ImplicitDocumentStart;
                Ok(Some(Event::StreamStart))
            }
            State::ImplicitDocumentStart => {
                if !self.check_token(&[
                    Choice::Directive,
                    Choice::DocumentStart,
                    Choice::StreamEnd,
                ])? {
                    self.tag_handles = default_tags();
                    self.states.push(State::DocumentEnd);
                    self.state = State::BlockNode;
                    Ok(Some(Event::DocumentStart))
                } else {
                    self.parse_document_start()
                }
            }
            State::DocumentStart => self.parse_document_start(),
            State::DocumentEnd => {
                if self.check_token(&[Choice::DocumentEnd])? {
                    self.get_token()?;
                }
                self.state = State::DocumentStart;
                Ok(Some(Event::DocumentEnd))
            }
            State::DocumentContent => {
                if self.check_token(&[
                    Choice::Directive,
                    Choice::DocumentStart,
                    Choice::DocumentEnd,
                    Choice::StreamEnd,
                ])? {
                    self.state = self.states.pop().unwrap_or(State::End);
                    Ok(Some(empty_scalar()))
                } else {
                    self.parse_node(true, false)
                }
            }
            State::BlockNode => self.parse_node(true, false),
            State::BlockNodeOrIndentlessSequence => self.parse_node(true, true),
            State::FlowNode => self.parse_node(false, false),
            State::BlockSequenceFirstEntry => {
                let token = self.get_token()?.expect("token");
                self.marks.push(token.start_mark);
                self.parse_block_sequence_entry()
            }
            State::BlockSequenceEntry => self.parse_block_sequence_entry(),
            State::IndentlessSequenceEntry => self.parse_indentless_sequence_entry(),
            State::BlockMappingFirstKey => {
                let token = self.get_token()?.expect("token");
                self.marks.push(token.start_mark);
                self.parse_block_mapping_key()
            }
            State::BlockMappingKey => self.parse_block_mapping_key(),
            State::BlockMappingValue => self.parse_block_mapping_value(),
            State::FlowSequenceFirstEntry => {
                let token = self.get_token()?.expect("token");
                self.marks.push(token.start_mark);
                self.parse_flow_sequence_entry(true)
            }
            State::FlowSequenceEntry { first } => self.parse_flow_sequence_entry(first),
            State::FlowSequenceEntryMappingKey => {
                self.get_token()?;
                if !self.check_token(&[
                    Choice::Value,
                    Choice::FlowEntry,
                    Choice::FlowSequenceEnd,
                ])? {
                    self.states.push(State::FlowSequenceEntryMappingValue);
                    self.state = State::FlowNode;
                    Ok(None)
                } else {
                    self.state = State::FlowSequenceEntryMappingValue;
                    Ok(Some(empty_scalar()))
                }
            }
            State::FlowSequenceEntryMappingValue => {
                if self.check_token(&[Choice::Value])? {
                    self.get_token()?;
                    if !self.check_token(&[Choice::FlowEntry, Choice::FlowSequenceEnd])? {
                        self.states.push(State::FlowSequenceEntryMappingEnd);
                        self.state = State::FlowNode;
                        Ok(None)
                    } else {
                        self.state = State::FlowSequenceEntryMappingEnd;
                        Ok(Some(empty_scalar()))
                    }
                } else {
                    self.state = State::FlowSequenceEntryMappingEnd;
                    Ok(Some(empty_scalar()))
                }
            }
            State::FlowSequenceEntryMappingEnd => {
                self.state = State::FlowSequenceEntry { first: false };
                Ok(Some(Event::MappingEnd))
            }
            State::FlowMappingFirstKey => {
                let token = self.get_token()?.expect("token");
                self.marks.push(token.start_mark);
                self.parse_flow_mapping_key(true)
            }
            State::FlowMappingKey { first } => self.parse_flow_mapping_key(first),
            State::FlowMappingValue => {
                if self.check_token(&[Choice::Value])? {
                    self.get_token()?;
                    if !self.check_token(&[Choice::FlowEntry, Choice::FlowMappingEnd])? {
                        self.states.push(State::FlowMappingKey { first: false });
                        self.state = State::FlowNode;
                        Ok(None)
                    } else {
                        self.state = State::FlowMappingKey { first: false };
                        Ok(Some(empty_scalar()))
                    }
                } else {
                    self.state = State::FlowMappingKey { first: false };
                    Ok(Some(empty_scalar()))
                }
            }
            State::FlowMappingEmptyValue => {
                self.state = State::FlowMappingKey { first: false };
                Ok(Some(empty_scalar()))
            }
        }
    }

    fn parse_document_start(&mut self) -> Result<Option<Event>> {
        // Skip extra document end indicators.
        while self.check_token(&[Choice::DocumentEnd])? {
            self.get_token()?;
        }

        if !self.check_token(&[Choice::StreamEnd])? {
            self.process_directives()?;
            if !self.check_token(&[Choice::DocumentStart])? {
                let token = self.peek_required()?;
                return Err(YamlError::new(
                    None,
                    None,
                    format!(
                        "expected '<document start>', but found {}",
                        py_repr(token.id())
                    ),
                    Some(token.start_mark),
                ));
            }
            self.get_token()?;
            self.states.push(State::DocumentEnd);
            self.state = State::DocumentContent;
            Ok(Some(Event::DocumentStart))
        } else {
            // End of the stream.
            self.get_token()?;
            self.state = State::End;
            Ok(Some(Event::StreamEnd))
        }
    }

    fn process_directives(&mut self) -> Result<()> {
        self.yaml_version = None;
        self.tag_handles.clear();
        while self.check_token(&[Choice::Directive])? {
            let token = self.get_token()?.expect("token");
            if let TokenKind::Directive { name, value } = &token.kind {
                if name == "YAML" {
                    if self.yaml_version.is_some() {
                        return Err(YamlError::new(
                            None,
                            None,
                            "found duplicate YAML directive".into(),
                            Some(token.start_mark),
                        ));
                    }
                    if let DirectiveValue::Yaml(major, minor) = value {
                        if *major != 1 {
                            return Err(YamlError::new(
                                None,
                                None,
                                "found incompatible YAML document (version 1.* is required)".into(),
                                Some(token.start_mark),
                            ));
                        }
                        self.yaml_version = Some((*major, *minor));
                    }
                } else if name == "TAG"
                    && let DirectiveValue::Tag(handle, prefix) = value
                {
                    if self.tag_handles.contains_key(handle) {
                        return Err(YamlError::new(
                            None,
                            None,
                            format!("duplicate tag handle {}", py_repr(handle)),
                            Some(token.start_mark),
                        ));
                    }
                    self.tag_handles.insert(handle.clone(), prefix.clone());
                }
            }
        }
        for (key, value) in default_tags() {
            self.tag_handles.entry(key).or_insert(value);
        }
        Ok(())
    }

    fn parse_node(&mut self, block: bool, indentless_sequence: bool) -> Result<Option<Event>> {
        if self.check_token(&[Choice::Alias])? {
            let token = self.get_token()?.expect("token");
            self.state = self.states.pop().unwrap_or(State::End);
            if let TokenKind::Alias(value) = token.kind {
                return Ok(Some(Event::Alias {
                    value,
                    mark: token.start_mark,
                }));
            }
            return Ok(None);
        }

        let mut anchor: Option<String> = None;
        let mut raw_tag: Option<(Option<String>, String)> = None;
        let mut start_mark: Option<Mark> = None;
        let mut tag_mark: Option<Mark> = None;

        if self.check_token(&[Choice::Anchor])? {
            let token = self.get_token()?.expect("token");
            start_mark = Some(token.start_mark);
            if let TokenKind::Anchor(value) = token.kind {
                anchor = Some(value);
            }
            if self.check_token(&[Choice::Tag])? {
                let token = self.get_token()?.expect("token");
                tag_mark = Some(token.start_mark);
                if let TokenKind::Tag { handle, suffix } = token.kind {
                    raw_tag = Some((handle, suffix));
                }
            }
        } else if self.check_token(&[Choice::Tag])? {
            let token = self.get_token()?.expect("token");
            start_mark = Some(token.start_mark);
            tag_mark = Some(token.start_mark);
            if let TokenKind::Tag { handle, suffix } = token.kind {
                raw_tag = Some((handle, suffix));
            }
            if self.check_token(&[Choice::Anchor])? {
                let token = self.get_token()?.expect("token");
                if let TokenKind::Anchor(value) = token.kind {
                    anchor = Some(value);
                }
            }
        }

        let tag: Option<String> = match raw_tag {
            Some((Some(handle), suffix)) => match self.tag_handles.get(&handle) {
                Some(prefix) => Some(format!("{prefix}{suffix}")),
                None => {
                    return Err(YamlError::new(
                        Some("while parsing a node"),
                        start_mark,
                        format!("found undefined tag handle {}", py_repr(&handle)),
                        tag_mark,
                    ));
                }
            },
            Some((None, suffix)) => Some(suffix),
            None => None,
        };

        let has_properties = anchor.is_some() || tag.is_some();

        if indentless_sequence && self.check_token(&[Choice::BlockEntry])? {
            self.state = State::IndentlessSequenceEntry;
            return Ok(Some(Event::SequenceStart { anchor, tag }));
        }

        if self.check_token(&[Choice::Scalar])? {
            let token = self.get_token()?.expect("token");
            self.state = self.states.pop().unwrap_or(State::End);
            if let TokenKind::Scalar {
                value,
                plain,
                style,
            } = token.kind
            {
                return Ok(Some(Event::Scalar {
                    value,
                    plain,
                    style,
                    anchor,
                    tag,
                }));
            }
            Ok(None)
        } else if self.check_token(&[Choice::FlowSequenceStart])? {
            self.state = State::FlowSequenceFirstEntry;
            Ok(Some(Event::SequenceStart { anchor, tag }))
        } else if self.check_token(&[Choice::FlowMappingStart])? {
            self.state = State::FlowMappingFirstKey;
            Ok(Some(Event::MappingStart { anchor, tag }))
        } else if block && self.check_token(&[Choice::BlockSequenceStart])? {
            self.state = State::BlockSequenceFirstEntry;
            Ok(Some(Event::SequenceStart { anchor, tag }))
        } else if block && self.check_token(&[Choice::BlockMappingStart])? {
            self.state = State::BlockMappingFirstKey;
            Ok(Some(Event::MappingStart { anchor, tag }))
        } else if has_properties {
            // Empty scalars are allowed even if a tag or an anchor is
            // specified.
            self.state = self.states.pop().unwrap_or(State::End);
            Ok(Some(Event::Scalar {
                value: String::new(),
                plain: true,
                style: None,
                anchor,
                tag,
            }))
        } else {
            let node = if block { "block" } else { "flow" };
            let token = self.peek_required()?;
            Err(YamlError::new(
                Some(&format!("while parsing a {node} node")),
                start_mark.or(Some(token.start_mark)),
                format!(
                    "expected the node content, but found {}",
                    py_repr(token.id())
                ),
                Some(token.start_mark),
            ))
        }
    }

    fn parse_block_sequence_entry(&mut self) -> Result<Option<Event>> {
        if self.check_token(&[Choice::BlockEntry])? {
            self.get_token()?;
            if !self.check_token(&[Choice::BlockEntry, Choice::BlockEnd])? {
                self.states.push(State::BlockSequenceEntry);
                self.state = State::BlockNode;
                return Ok(None);
            }
            self.state = State::BlockSequenceEntry;
            return Ok(Some(empty_scalar()));
        }
        if !self.check_token(&[Choice::BlockEnd])? {
            let token = self.peek_required()?;
            let context_mark = self.marks.last().copied();
            return Err(YamlError::new(
                Some("while parsing a block collection"),
                context_mark,
                format!("expected <block end>, but found {}", py_repr(token.id())),
                Some(token.start_mark),
            ));
        }
        self.get_token()?;
        self.state = self.states.pop().unwrap_or(State::End);
        self.marks.pop();
        Ok(Some(Event::SequenceEnd))
    }

    fn parse_indentless_sequence_entry(&mut self) -> Result<Option<Event>> {
        if self.check_token(&[Choice::BlockEntry])? {
            self.get_token()?;
            if !self.check_token(&[
                Choice::BlockEntry,
                Choice::Key,
                Choice::Value,
                Choice::BlockEnd,
            ])? {
                self.states.push(State::IndentlessSequenceEntry);
                self.state = State::BlockNode;
                return Ok(None);
            }
            self.state = State::IndentlessSequenceEntry;
            return Ok(Some(empty_scalar()));
        }
        self.state = self.states.pop().unwrap_or(State::End);
        Ok(Some(Event::SequenceEnd))
    }

    fn parse_block_mapping_key(&mut self) -> Result<Option<Event>> {
        if self.check_token(&[Choice::Key])? {
            self.get_token()?;
            if !self.check_token(&[Choice::Key, Choice::Value, Choice::BlockEnd])? {
                self.states.push(State::BlockMappingValue);
                self.state = State::BlockNodeOrIndentlessSequence;
                return Ok(None);
            }
            self.state = State::BlockMappingValue;
            return Ok(Some(empty_scalar()));
        }
        if !self.check_token(&[Choice::BlockEnd])? {
            let token = self.peek_required()?;
            let context_mark = self.marks.last().copied();
            return Err(YamlError::new(
                Some("while parsing a block mapping"),
                context_mark,
                format!("expected <block end>, but found {}", py_repr(token.id())),
                Some(token.start_mark),
            ));
        }
        self.get_token()?;
        self.state = self.states.pop().unwrap_or(State::End);
        self.marks.pop();
        Ok(Some(Event::MappingEnd))
    }

    fn parse_block_mapping_value(&mut self) -> Result<Option<Event>> {
        if self.check_token(&[Choice::Value])? {
            self.get_token()?;
            if !self.check_token(&[Choice::Key, Choice::Value, Choice::BlockEnd])? {
                self.states.push(State::BlockMappingKey);
                self.state = State::BlockNodeOrIndentlessSequence;
                return Ok(None);
            }
            self.state = State::BlockMappingKey;
            Ok(Some(empty_scalar()))
        } else {
            self.state = State::BlockMappingKey;
            Ok(Some(empty_scalar()))
        }
    }

    fn parse_flow_sequence_entry(&mut self, first: bool) -> Result<Option<Event>> {
        if !self.check_token(&[Choice::FlowSequenceEnd])? {
            if !first {
                if self.check_token(&[Choice::FlowEntry])? {
                    self.get_token()?;
                } else {
                    let token = self.peek_required()?;
                    let context_mark = self.marks.last().copied();
                    return Err(YamlError::new(
                        Some("while parsing a flow sequence"),
                        context_mark,
                        format!("expected ',' or ']', but got {}", py_repr(token.id())),
                        Some(token.start_mark),
                    ));
                }
            }
            if self.check_token(&[Choice::Key])? {
                self.state = State::FlowSequenceEntryMappingKey;
                return Ok(Some(Event::MappingStart {
                    anchor: None,
                    tag: None,
                }));
            } else if !self.check_token(&[Choice::FlowSequenceEnd])? {
                self.states.push(State::FlowSequenceEntry { first: false });
                self.state = State::FlowNode;
                return Ok(None);
            }
        }
        self.get_token()?;
        self.state = self.states.pop().unwrap_or(State::End);
        self.marks.pop();
        Ok(Some(Event::SequenceEnd))
    }

    fn parse_flow_mapping_key(&mut self, first: bool) -> Result<Option<Event>> {
        if !self.check_token(&[Choice::FlowMappingEnd])? {
            if !first {
                if self.check_token(&[Choice::FlowEntry])? {
                    self.get_token()?;
                } else {
                    let token = self.peek_required()?;
                    let context_mark = self.marks.last().copied();
                    return Err(YamlError::new(
                        Some("while parsing a flow mapping"),
                        context_mark,
                        format!("expected ',' or '}}', but got {}", py_repr(token.id())),
                        Some(token.start_mark),
                    ));
                }
            }
            if self.check_token(&[Choice::Key])? {
                self.get_token()?;
                if !self.check_token(&[Choice::Value, Choice::FlowEntry, Choice::FlowMappingEnd])? {
                    self.states.push(State::FlowMappingValue);
                    self.state = State::FlowNode;
                    return Ok(None);
                }
                self.state = State::FlowMappingValue;
                return Ok(Some(empty_scalar()));
            } else if !self.check_token(&[Choice::FlowMappingEnd])? {
                self.states.push(State::FlowMappingEmptyValue);
                self.state = State::FlowNode;
                return Ok(None);
            }
        }
        self.get_token()?;
        self.state = self.states.pop().unwrap_or(State::End);
        self.marks.pop();
        Ok(Some(Event::MappingEnd))
    }
}

fn default_tags() -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert("!".to_string(), "!".to_string());
    map.insert("!!".to_string(), "tag:yaml.org,2002:".to_string());
    map
}
