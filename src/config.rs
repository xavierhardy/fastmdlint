//! Configuration loading and resolution, compatible with markdownlint-cli.
//!
//! Config files may be JSON, JSONC, YAML or TOML. Resolution mirrors
//! markdownlint's `getEffectiveConfig`: a `default` key sets the baseline,
//! then each key (rule id, alias, or tag) enables/disables rules and supplies
//! options, in document order.

use std::collections::HashMap;
use std::path::Path;

use serde_json::{Map, Value};

use crate::rules::{RULES, RuleMeta, Severity};

/// A parsed, merged configuration object.
#[derive(Debug, Clone)]
pub struct Config {
    pub raw: Value,
}

/// Effective per-rule configuration.
#[derive(Debug, Clone)]
pub struct RuleConfig {
    pub enabled: bool,
    pub severity: Severity,
    pub options: Value,
}

/// Resolved configuration: primary rule id -> [`RuleConfig`].
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub rules: HashMap<&'static str, RuleConfig>,
}

impl ResolvedConfig {
    pub fn get(&self, id: &str) -> Option<&RuleConfig> {
        self.rules.get(id)
    }
}

/// Build the alias/tag -> rule-ids map (upper-cased keys), like
/// `mapAliasToRuleNames`.
fn alias_map() -> HashMap<String, Vec<&'static str>> {
    let mut map: HashMap<String, Vec<&'static str>> = HashMap::new();
    for rule in RULES {
        let id = rule.names[0];
        for name in rule.names {
            map.entry(name.to_uppercase()).or_default().push(id);
        }
        for tag in rule.tags {
            map.entry(tag.to_uppercase()).or_default().push(id);
        }
    }
    for v in map.values_mut() {
        let mut seen = std::collections::HashSet::new();
        v.retain(|x| seen.insert(*x));
    }
    map
}

impl Config {
    pub fn empty() -> Config {
        Config {
            raw: Value::Object(Map::new()),
        }
    }

    pub fn from_value(raw: Value) -> Config {
        Config { raw }
    }

    /// Load a config file, trying JSONC, TOML then YAML parsers, resolving
    /// `extends` relative to the file.
    pub fn from_file(path: &Path) -> Result<Config, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Cannot read config file '{}': {e}", path.display()))?;
        let value = parse_config_content(&content)
            .map_err(|e| format!("Cannot parse config file '{}': {e}", path.display()))?;
        let value = resolve_extends(value, path)?;
        Ok(Config { raw: value })
    }

    /// Apply `--enable` / `--disable` overrides, like the CLI does.
    pub fn apply_enable_disable(&mut self, enable: &[String], disable: &[String]) {
        let obj = match &mut self.raw {
            Value::Object(m) => m,
            _ => return,
        };
        for rule in enable {
            let falsy = obj.get(rule).map(is_falsy).unwrap_or(true);
            if falsy {
                obj.insert(rule.clone(), Value::Bool(true));
            }
        }
        for rule in disable {
            obj.insert(rule.clone(), Value::Bool(false));
        }
    }

    /// Compute the effective per-rule configuration.
    pub fn resolve(&self) -> ResolvedConfig {
        let aliases = alias_map();
        let obj = self.raw.as_object().cloned().unwrap_or_default();

        let mut default_enable = true;
        let mut default_severity = Severity::Error;
        for (key, value) in &obj {
            if key.to_uppercase() == "DEFAULT" {
                default_enable = !is_falsy(value);
                if value.as_str() == Some("warning") {
                    default_severity = Severity::Warning;
                }
                break;
            }
        }

        let mut rules: HashMap<&'static str, RuleConfig> = HashMap::new();
        for rule in RULES {
            rules.insert(
                rule.names[0],
                RuleConfig {
                    enabled: default_enable,
                    severity: default_severity,
                    options: Value::Object(Map::new()),
                },
            );
        }

        for (key, value) in &obj {
            let key_upper = key.to_uppercase();
            let (enabled, severity, options) = interpret_value(value);
            if let Some(ids) = aliases.get(&key_upper) {
                for id in ids {
                    rules.insert(
                        id,
                        RuleConfig {
                            enabled,
                            severity,
                            options: options.clone(),
                        },
                    );
                }
            }
        }

        ResolvedConfig { rules }
    }
}

fn is_falsy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => !b,
        Value::Null => true,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        _ => false,
    }
}

fn interpret_value(value: &Value) -> (bool, Severity, Value) {
    if is_falsy(value) {
        return (false, Severity::Error, Value::Object(Map::new()));
    }
    match value {
        Value::Object(map) => {
            let enabled = match map.get("enabled") {
                Some(v) => !is_falsy(v),
                None => true,
            };
            let severity = if map.get("severity").and_then(|v| v.as_str()) == Some("warning") {
                Severity::Warning
            } else {
                Severity::Error
            };
            let mut opts = map.clone();
            opts.remove("enabled");
            opts.remove("severity");
            (enabled, severity, Value::Object(opts))
        }
        _ => {
            let severity = if value.as_str() == Some("warning") {
                Severity::Warning
            } else {
                Severity::Error
            };
            (true, severity, Value::Object(Map::new()))
        }
    }
}

/// Parse config content, trying JSONC, then TOML, then YAML.
pub fn parse_config_content(content: &str) -> Result<Value, String> {
    if let Ok(v) = parse_jsonc(content) {
        if v.is_object() {
            return Ok(v);
        }
    }
    if let Ok(v) = toml::from_str::<Value>(content) {
        if v.is_object() && v.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
            return Ok(v);
        }
    }
    match parse_yaml(content) {
        Ok(v) if v.is_object() => Ok(v),
        Ok(_) => Ok(Value::Object(Map::new())),
        Err(e) => Err(format!("not valid JSON/JSONC, TOML or YAML: {e}")),
    }
}

fn parse_jsonc(content: &str) -> Result<Value, String> {
    let stripped = strip_jsonc(content);
    serde_json::from_str(&stripped).map_err(|e| e.to_string())
}

/// Strip `//` and `/* */` comments and trailing commas from JSONC.
fn strip_jsonc(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut in_string = false;
    let mut escape = false;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_string = true;
            out.push(c);
            i += 1;
        } else if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else {
            out.push(c);
            i += 1;
        }
    }
    remove_trailing_commas(&out)
}

fn remove_trailing_commas(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ',' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && (chars[j] == '}' || chars[j] == ']') {
                i += 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Parse YAML into a JSON value using the internal YAML loader.
pub fn parse_yaml(content: &str) -> Result<Value, String> {
    let yaml = crate::pyyaml::loader::load(content).map_err(|e| format!("{e:?}"))?;
    Ok(yaml_to_json(&yaml))
}

fn yaml_to_json(v: &crate::pyyaml::value::YamlValue) -> Value {
    use crate::pyyaml::value::YamlValue as Y;
    match v {
        Y::Null => Value::Null,
        Y::Bool(b) => Value::Bool(*b),
        Y::Int(i) => Value::Number((*i).into()),
        Y::Float(f) => serde_json::Number::from_f64(*f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Y::Str(s) => Value::String(s.clone()),
        Y::Seq(items) => Value::Array(items.iter().map(yaml_to_json).collect()),
        Y::Map(m) => {
            let mut obj = Map::new();
            for (k, val) in m.iter() {
                let key = match k {
                    Y::Str(s) => s.clone(),
                    Y::Int(i) => i.to_string(),
                    Y::Bool(b) => b.to_string(),
                    _ => continue,
                };
                obj.insert(key, yaml_to_json(val));
            }
            Value::Object(obj)
        }
    }
}

/// Resolve a config's `extends` key by merging the referenced base config.
fn resolve_extends(mut value: Value, path: &Path) -> Result<Value, String> {
    let extends = value
        .as_object()
        .and_then(|o| o.get("extends"))
        .and_then(|v| v.as_str())
        .map(String::from);
    if let Some(rel) = extends {
        let base_path = expand_and_resolve(&rel, path);
        let base = Config::from_file(&base_path)?.raw;
        if let Value::Object(obj) = &mut value {
            obj.remove("extends");
        }
        let merged = deep_merge(base, value);
        return Ok(merged);
    }
    Ok(value)
}

fn expand_and_resolve(rel: &str, config_path: &Path) -> std::path::PathBuf {
    if let Some(stripped) = rel.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return Path::new(&home).join(stripped);
        }
    }
    let p = Path::new(rel);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        config_path.parent().unwrap_or(Path::new(".")).join(p)
    }
}

/// Deep-merge `over` onto `base` (over wins), like `deep-extend`.
pub fn deep_merge(base: Value, over: Value) -> Value {
    match (base, over) {
        (Value::Object(mut b), Value::Object(o)) => {
            for (k, v) in o {
                let merged = match b.remove(&k) {
                    Some(existing) => deep_merge(existing, v),
                    None => v,
                };
                b.insert(k, merged);
            }
            Value::Object(b)
        }
        (_, over) => over,
    }
}

impl RuleMeta {
    /// Look up this rule's resolved config, if enabled.
    pub fn resolved<'a>(&self, cfg: &'a ResolvedConfig) -> Option<&'a RuleConfig> {
        cfg.get(self.names[0]).filter(|rc| rc.enabled)
    }
}
