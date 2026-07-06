//! Implicit tag resolution for plain scalars (YAML 1.1 semantics), plus an
//! extra integer pattern covering `0o` octal forms used by the
//! `quoted-strings` rule.

use std::sync::LazyLock;

use regex::Regex;

pub const DEFAULT_SCALAR_TAG: &str = "tag:yaml.org,2002:str";

struct ImplicitResolver {
    tag: &'static str,
    regex: Regex,
    first: &'static [char],
    /// Matches the empty string (PyYAML keys these under `''`).
    matches_empty: bool,
}

static RESOLVERS: LazyLock<Vec<ImplicitResolver>> = LazyLock::new(|| {
    vec![
        ImplicitResolver {
            tag: "tag:yaml.org,2002:bool",
            regex: Regex::new(
                r"^(?:yes|Yes|YES|no|No|NO|true|True|TRUE|false|False|FALSE|on|On|ON|off|Off|OFF)$",
            )
            .unwrap(),
            first: &[
                'y', 'Y', 'n', 'N', 't', 'T', 'f', 'F', 'o', 'O',
            ],
            matches_empty: false,
        },
        ImplicitResolver {
            tag: "tag:yaml.org,2002:float",
            regex: Regex::new(
                r"^(?:[-+]?(?:[0-9][0-9_]*)\.[0-9_]*(?:[eE][-+][0-9]+)?|\.[0-9][0-9_]*(?:[eE][-+][0-9]+)?|[-+]?[0-9][0-9_]*(?::[0-5]?[0-9])+\.[0-9_]*|[-+]?\.(?:inf|Inf|INF)|\.(?:nan|NaN|NAN))$",
            )
            .unwrap(),
            first: &['-', '+', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '.'],
            matches_empty: false,
        },
        ImplicitResolver {
            tag: "tag:yaml.org,2002:int",
            regex: Regex::new(
                r"^(?:[-+]?0b[0-1_]+|[-+]?0[0-7_]+|[-+]?(?:0|[1-9][0-9_]*)|[-+]?0x[0-9a-fA-F_]+|[-+]?[1-9][0-9_]*(?::[0-5]?[0-9])+)$",
            )
            .unwrap(),
            first: &['-', '+', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9'],
            matches_empty: false,
        },
        ImplicitResolver {
            tag: "tag:yaml.org,2002:merge",
            regex: Regex::new(r"^(?:<<)$").unwrap(),
            first: &['<'],
            matches_empty: false,
        },
        ImplicitResolver {
            tag: "tag:yaml.org,2002:null",
            regex: Regex::new(r"^(?:~|null|Null|NULL|)$").unwrap(),
            first: &['~', 'n', 'N'],
            matches_empty: true,
        },
        ImplicitResolver {
            tag: "tag:yaml.org,2002:timestamp",
            regex: Regex::new(
                r"^(?:[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]|[0-9][0-9][0-9][0-9]-[0-9][0-9]?-[0-9][0-9]?(?:[Tt]|[ \t]+)[0-9][0-9]?:[0-9][0-9]:[0-9][0-9](?:\.[0-9]*)?(?:[ \t]*(?:Z|[-+][0-9][0-9]?(?::[0-9][0-9])?))?)$",
            )
            .unwrap(),
            first: &['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'],
            matches_empty: false,
        },
        ImplicitResolver {
            tag: "tag:yaml.org,2002:value",
            regex: Regex::new(r"^(?:=)$").unwrap(),
            first: &['='],
            matches_empty: false,
        },
        ImplicitResolver {
            tag: "tag:yaml.org,2002:yaml",
            regex: Regex::new(r"^(?:!|&|\*)$").unwrap(),
            first: &['!', '&', '*'],
            matches_empty: false,
        },
        // Extra integer resolver extending int with 0o octal forms (appended
        // last, so it only matters for values the default pattern misses).
        ImplicitResolver {
            tag: "tag:yaml.org,2002:int",
            regex: Regex::new(
                r"^(?:[-+]?0b[0-1_]+|[-+]?0o?[0-7_]+|[-+]?0[0-7_]+|[-+]?(?:0|[1-9][0-9_]*)|[-+]?0x[0-9a-fA-F_]+|[-+]?[1-9][0-9_]*(?::[0-5]?[0-9])+)$",
            )
            .unwrap(),
            first: &['-', '+', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9'],
            matches_empty: false,
        },
    ]
});

/// Resolve the implicit tag of a plain scalar value.
pub fn resolve_scalar_tag(value: &str) -> &'static str {
    let first = value.chars().next();
    for resolver in RESOLVERS.iter() {
        let applicable = match first {
            None => resolver.matches_empty,
            Some(ch) => resolver.first.contains(&ch),
        };
        if applicable && resolver.regex.is_match(value) {
            return resolver.tag;
        }
    }
    DEFAULT_SCALAR_TAG
}
