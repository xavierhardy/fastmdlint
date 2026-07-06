//! Load a YAML document into a [`YamlValue`] tree, resolving plain scalars
//! to booleans, integers, floats and nulls with YAML 1.1 semantics (the
//! same resolution rules as the token resolver in this crate).

use std::collections::HashMap;

use super::error::YamlError;
use super::parser::{Event, Parser};
use super::resolver::resolve_scalar_tag;
use super::value::{YamlMapping, YamlValue};

type Result<T> = std::result::Result<T, YamlError>;

fn plain_error(problem: String) -> YamlError {
    YamlError::new(None, None, problem, None)
}

fn construct_bool(value: &str) -> YamlValue {
    match value {
        "yes" | "Yes" | "YES" | "true" | "True" | "TRUE" | "on" | "On" | "ON" => {
            YamlValue::Bool(true)
        }
        _ => YamlValue::Bool(false),
    }
}

fn construct_int(value: &str) -> Option<YamlValue> {
    let cleaned: String = value.chars().filter(|&c| c != '_').collect();
    let (sign, digits) = match cleaned.strip_prefix('-') {
        Some(rest) => (-1i64, rest),
        None => (1i64, cleaned.strip_prefix('+').unwrap_or(&cleaned)),
    };
    let parsed = if let Some(rest) = digits.strip_prefix("0b") {
        i64::from_str_radix(rest, 2).ok()
    } else if let Some(rest) = digits.strip_prefix("0x") {
        i64::from_str_radix(rest, 16).ok()
    } else if let Some(rest) = digits.strip_prefix("0o") {
        i64::from_str_radix(rest, 8).ok()
    } else if digits.len() > 1 && digits.starts_with('0') && !digits.contains(':') {
        i64::from_str_radix(digits, 8).ok()
    } else if digits.contains(':') {
        // Sexagesimal, e.g. 190:20:30.
        let mut total: i64 = 0;
        for part in digits.split(':') {
            total = total.checked_mul(60)?.checked_add(part.parse().ok()?)?;
        }
        Some(total)
    } else {
        digits.parse::<i64>().ok()
    };
    parsed.map(|n| YamlValue::Int(sign * n))
}

fn construct_float(value: &str) -> Option<YamlValue> {
    let cleaned: String = value.chars().filter(|&c| c != '_').collect();
    let lower = cleaned.to_lowercase();
    if lower.ends_with(".inf") {
        let sign = if lower.starts_with('-') { -1.0 } else { 1.0 };
        return Some(YamlValue::Float(sign * f64::INFINITY));
    }
    if lower.ends_with(".nan") {
        return Some(YamlValue::Float(f64::NAN));
    }
    if cleaned.contains(':') {
        // Sexagesimal float: rare and unused in configurations.
        return None;
    }
    cleaned.parse::<f64>().ok().map(YamlValue::Float)
}

fn construct_scalar(value: String, plain: bool, tag: Option<&str>) -> YamlValue {
    let resolved_tag = match tag {
        Some(tag) => tag.to_string(),
        None => {
            if plain {
                resolve_scalar_tag(&value).to_string()
            } else {
                "tag:yaml.org,2002:str".to_string()
            }
        }
    };
    match resolved_tag.as_str() {
        "tag:yaml.org,2002:null" => YamlValue::Null,
        "tag:yaml.org,2002:bool" => construct_bool(&value),
        "tag:yaml.org,2002:int" => construct_int(&value).unwrap_or(YamlValue::Str(value)),
        "tag:yaml.org,2002:float" => construct_float(&value).unwrap_or(YamlValue::Str(value)),
        _ => YamlValue::Str(value),
    }
}

struct Loader {
    parser: Parser,
    anchors: HashMap<String, YamlValue>,
}

impl Loader {
    fn compose(&mut self, event: Event) -> Result<YamlValue> {
        match event {
            Event::Alias { value, mark } => match self.anchors.get(&value) {
                Some(node) => Ok(node.clone()),
                None => Err(YamlError::new(
                    None,
                    None,
                    format!("found undefined alias {value:?}"),
                    Some(mark),
                )),
            },
            Event::Scalar {
                value,
                plain,
                style: _,
                anchor,
                tag,
            } => {
                let node = construct_scalar(value, plain, tag.as_deref());
                if let Some(anchor) = anchor {
                    self.anchors.insert(anchor, node.clone());
                }
                Ok(node)
            }
            Event::SequenceStart { anchor, tag: _ } => {
                let mut items = Vec::new();
                loop {
                    match self.next_event()? {
                        Event::SequenceEnd => break,
                        event => items.push(self.compose(event)?),
                    }
                }
                let node = YamlValue::Seq(items);
                if let Some(anchor) = anchor {
                    self.anchors.insert(anchor, node.clone());
                }
                Ok(node)
            }
            Event::MappingStart { anchor, tag: _ } => {
                let mut map = YamlMapping::new();
                loop {
                    let key = match self.next_event()? {
                        Event::MappingEnd => break,
                        event => self.compose(event)?,
                    };
                    let value_event = self.next_event()?;
                    let value = self.compose(value_event)?;
                    map.insert(key, value);
                }
                let node = YamlValue::Map(map);
                if let Some(anchor) = anchor {
                    self.anchors.insert(anchor, node.clone());
                }
                Ok(node)
            }
            other => Err(plain_error(format!(
                "unexpected event while composing: {other:?}"
            ))),
        }
    }

    fn next_event(&mut self) -> Result<Event> {
        match self.parser.next_event()? {
            Some(event) => Ok(event),
            None => Err(plain_error("unexpected end of event stream".to_string())),
        }
    }
}

/// Load a single YAML document. An empty stream loads as `Null`; multiple
/// documents are an error (like `yaml.safe_load`).
pub fn load(content: &str) -> Result<YamlValue> {
    let mut loader = Loader {
        parser: Parser::new(content),
        anchors: HashMap::new(),
    };

    match loader.next_event()? {
        Event::StreamStart => {}
        other => {
            return Err(plain_error(format!("expected stream start, got {other:?}")));
        }
    }

    let mut document: Option<YamlValue> = None;
    loop {
        match loader.parser.next_event()? {
            None | Some(Event::StreamEnd) => break,
            Some(Event::DocumentStart) => {
                let event = loader.next_event()?;
                let value = loader.compose(event)?;
                if document.is_some() {
                    return Err(plain_error(
                        "expected a single document in the stream".to_string(),
                    ));
                }
                document = Some(value);
            }
            Some(Event::DocumentEnd) => {
                loader.anchors.clear();
            }
            Some(other) => {
                return Err(plain_error(format!(
                    "unexpected event between documents: {other:?}"
                )));
            }
        }
    }

    Ok(document.unwrap_or(YamlValue::Null))
}
