//! A plain YAML value tree, used for loading configuration files.

/// An ordered mapping (insertion order is preserved; duplicate keys keep the
/// last value, like a Python dict built from a YAML document).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct YamlMapping {
    entries: Vec<(YamlValue, YamlValue)>,
}

impl YamlMapping {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<&YamlValue> {
        self.entries
            .iter()
            .find(|(k, _)| k.as_str() == Some(key))
            .map(|(_, v)| v)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    pub fn insert(&mut self, key: YamlValue, value: YamlValue) {
        if let Some(entry) = self.entries.iter_mut().find(|(k, _)| *k == key) {
            entry.1 = value;
        } else {
            self.entries.push((key, value));
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&YamlValue, &YamlValue)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum YamlValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Seq(Vec<YamlValue>),
    Map(YamlMapping),
}

impl YamlValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            YamlValue::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            YamlValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            YamlValue::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_sequence(&self) -> Option<&[YamlValue]> {
        match self {
            YamlValue::Seq(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_mapping(&self) -> Option<&YamlMapping> {
        match self {
            YamlValue::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn is_bool(&self) -> bool {
        matches!(self, YamlValue::Bool(_))
    }

    pub fn is_string(&self) -> bool {
        matches!(self, YamlValue::Str(_))
    }
}
