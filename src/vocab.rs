//! Typed cue vocabularies: a Lamlet package of attribute functions, generated from a host's list of keys.
//!
//! A cue's attributes are key and value Strings (`content`'s `Attr`). A host that knows its keys (the parameters
//! a cue may set, and their kinds) describes them in JSON, and [`package`] writes one typed function per key.
//! A misspelled key is then an unknown name and a wrong value a type error, both reported at the line of the
//! presentation before any speech is synthesized.
//!
//! ```json
//! [{"name": "intensity", "kind": "float"},
//!  {"name": "camera.yaw", "kind": "float"},
//!  {"name": "mode", "kind": "text", "options": ["albedo", "light-only"]},
//!  {"name": "label", "kind": "text", "key": "ui.label"}]
//! ```
//!
//! | Kind | Functions (for `intensity`) | Attribute value |
//! |---|---|---|
//! | `float` | `{intensity 0.5}`, `{intensityRange 0.0 1.0}` | `0.5`, `0.0..1.0` (from one value to the other over a cue's words) |
//! | `int` | `{steps 3}` | `3` |
//! | `bool` | `{visible true}` | `true` |
//! | `vector` | `{tint {' 1.0 0.5 0.2}}` | `1.0,0.5,0.2` |
//! | `text` | `{mode "albedo"}`, and with options `modeAlbedo`, `modeLightOnly` | `albedo` |
//!
//! A function's name is the key in camel case (`camera.yaw` is `cameraYaw`); `"key"` sets the attribute key when
//! it should differ from the name.

use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Float,
    Int,
    Bool,
    Vector,
    Text { options: Vec<String> },
}

/// One key a cue may carry.
#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    /// The name the functions are named after.
    pub name: String,
    /// The attribute key written in cues (the name, unless given).
    pub key: String,
    pub kind: Kind,
}

/// Keys from their JSON description.
pub fn keys_from_json(source: &str) -> Result<Vec<Key>, String> {
    let json: serde_json::Value = serde_json::from_str(source).map_err(|e| format!("the key list is not JSON: {e}"))?;
    let items = json.as_array().ok_or("the key list is a JSON array")?;
    items
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let at = |m: &str| format!("key {}: {m}", i + 1);
            let name = p.get("name").and_then(|n| n.as_str()).ok_or_else(|| at("needs a \"name\" string"))?;
            let key = match p.get("key") {
                None => name,
                Some(k) => k.as_str().ok_or_else(|| at("\"key\" is a string"))?,
            };
            let options = match p.get("options") {
                None => vec![],
                Some(o) => o
                    .as_array()
                    .and_then(|o| o.iter().map(|x| x.as_str().map(String::from)).collect::<Option<Vec<_>>>())
                    .ok_or_else(|| at("\"options\" is a list of strings"))?,
            };
            let has_options = !options.is_empty();
            let kind = match p.get("kind").and_then(|k| k.as_str()) {
                Some("float") => Kind::Float,
                Some("int") => Kind::Int,
                Some("bool") => Kind::Bool,
                Some("vector") => Kind::Vector,
                Some("text") => Kind::Text { options },
                Some(other) => return Err(at(&format!("unknown kind {other:?}; it is float, int, bool, vector, or text"))),
                None => return Err(at("needs a \"kind\" string")),
            };
            if has_options && !matches!(kind, Kind::Text { .. }) {
                return Err(at("only a text key has \"options\""));
            }
            if key.is_empty() || key.chars().any(|c| c.is_whitespace() || c == '"') {
                return Err(at(&format!("{key:?} is not a key: one word without quotes")));
            }
            Ok(Key { name: name.to_string(), key: key.to_string(), kind })
        })
        .collect()
}

/// A Lamlet name from parts of a key: `camera.yaw` is `cameraYaw`, `light-only` is `lightOnly`.
pub fn camel(parts: &[&str]) -> Result<String, String> {
    let mut out = String::new();
    for word in parts.iter().flat_map(|p| p.split(['.', '-', '_', ' ', '/'])).filter(|w| !w.is_empty()) {
        if !word.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(format!("{word:?} cannot be part of a Lamlet name"));
        }
        let mut chars = word.chars();
        let first = chars.next().unwrap();
        if out.is_empty() {
            out.extend(first.to_lowercase());
        } else {
            out.extend(first.to_uppercase());
        }
        out.push_str(chars.as_str());
    }
    match out.chars().next() {
        Some(c) if c.is_ascii_alphabetic() => Ok(out),
        _ => Err(format!("{:?} gives no Lamlet name that starts with a letter", parts.join(""))),
    }
}

fn string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The Lamlet source of a vocabulary package. Fails when two functions would have the same name.
pub fn package(title: &str, keys: &[Key]) -> Result<String, String> {
    let mut out = format!(
        "{}\n\nGenerated by `lamlet-present vocab`; do not edit. Each function gives a cue attribute (`content`'s `Attr`).\n\n\
         $[Import* std]$[Import* content]\n",
        title.replace('$', "$$")
    );
    let mut names = BTreeSet::new();
    let mut define = |out: &mut String, name: String, doc: String, body: String| -> Result<(), String> {
        if !names.insert(name.clone()) {
            return Err(format!("two functions would be named `{name}`"));
        }
        // A function body `{^ {params} rest}` is written as a definition, `[# {name params} rest]`.
        let line = match body.strip_prefix("{^ {").and_then(|r| r.split_once("} ")) {
            Some((params, rest)) => format!("$[# {{{name} {params}}} {}]", &rest[..rest.len() - 1]),
            None => format!("$[# {name} {body}]"),
        };
        out.push_str(&format!("\n{}\n{line}\n", doc.replace('$', "$$")));
        Ok(())
    };
    for k in keys {
        let base = camel(&[&k.name])?;
        let key = string(&k.key);
        match &k.kind {
            Kind::Float => {
                define(&mut out, base.clone(), format!("`{}`, a number.", k.key), format!("{{^ {{x}} {{attr {key} {{str {{: x Float}}}}}}}}"))?;
                define(
                    &mut out,
                    format!("{base}Range"),
                    format!("`{}` from `a` to `b` over a cue's words.", k.key),
                    format!("{{^ {{a b}} {{attr {key} {{++ {{str {{: a Float}}}} {{++ \"..\" {{str {{: b Float}}}}}}}}}}}}"),
                )?;
            }
            Kind::Int => define(&mut out, base, format!("`{}`, an integer.", k.key), format!("{{^ {{n}} {{attr {key} {{str {{: n Int}}}}}}}}"))?,
            Kind::Bool => define(&mut out, base, format!("`{}`, on or off.", k.key), format!("{{^ {{b}} {{attr {key} {{!if {{: b Bool}} \"true\" \"false\"}}}}}}"))?,
            Kind::Vector => define(
                &mut out,
                base,
                format!("`{}`, a vector of numbers.", k.key),
                format!("{{^ {{xs}} {{attr {key} {{join \",\" {{map {{: xs {{List Float}}}} str}}}}}}}}"),
            )?,
            Kind::Text { options } => {
                define(&mut out, base.clone(), format!("`{}`, a text value.", k.key), format!("{{^ {{s}} {{attr {key} {{: s String}}}}}}"))?;
                for o in options {
                    define(&mut out, camel(&[&base, o])?, format!("`{}` set to `{}`.", k.key, o), format!("[% {{attr {key} {}}}]", string(o)))?;
                }
            }
        }
    }
    Ok(out)
}
