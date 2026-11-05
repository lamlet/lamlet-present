//! Typed cue vocabularies and attribute schemas, from a host's list of keys.
//!
//! A cue's attributes, and a heading's, are keys with values (`content`'s `Attr`). A host that knows its keys (the
//! parameters a cue may set, and their kinds) describes them in JSON. From that list:
//!
//! - [`package`] writes a Lamlet package with one typed function per cue key, so a misspelled key is an unknown
//!   name and a wrong value a type error, both reported at the presentation's line before any speech is
//!   synthesized. Each function writes its value as typed data (`toData`), not as a String.
//! - [`Schema`] checks what the functions cannot: a cue attribute written with `{attr …}` directly, and a
//!   heading's `{#id key=value}` attributes, which it reads by their key's kind and completes with defaults
//!   ([`crate::Presentation::apply_keys`]).
//!
//! ```json
//! [{"name": "intensity", "kind": "float"},
//!  {"name": "camera.yaw", "kind": "float"},
//!  {"name": "mode", "kind": "text", "options": ["albedo", "light-only"], "closed": true},
//!  {"name": "label", "kind": "text", "key": "ui.label"},
//!  {"name": "at", "kind": "type", "type": "{Struct {' {Field \"x\" Float} {Field \"y\" Float}}}"},
//!  {"name": "param", "kind": "float", "prefix": true},
//!  {"name": "target", "kind": "int", "on": "heading", "levels": [2], "default": 60}]
//! ```
//!
//! | Kind | Functions (for `intensity`) | Attribute value |
//! |---|---|---|
//! | `float` | `{intensity 0.5}`, `{intensityRange 0.0 1.0}` | a Float; a range is `{. from 0.0 to 1.0}` (from one value to the other over a cue's words) |
//! | `int` | `{steps 3}` | an Int |
//! | `bool` | `{visible true}` | a Bool |
//! | `vector` | `{tint {' 1.0 0.5 0.2}}` | a `List Float` |
//! | `text` | `{mode "albedo"}`, and with options `modeAlbedo`, `modeLightOnly` | a String |
//! | `names` | `{sources {' "a" "b"}}` | a `List String` |
//! | `type` | `{at {. x 1.0 y 2.0}}` | a value of the Lamlet type `"type"` (a static type expression) |
//! | `any` | none | anything: a key the host reserves for its own functions |
//!
//! A function's name is the key in camel case (`camera.yaw` is `cameraYaw`); `"key"` sets the attribute key when
//! it should differ from the name. Other fields:
//!
//! - `"options"` (text, names): the values a key takes. A text key gets one constant per option.
//! - `"closed": true` (text, names): only the options. A closed text key has no function that takes any String,
//!   so a wrong option is an unknown name; [`Schema`] refuses any other value.
//! - `"imports"` (type): packages the type expression needs, imported with `Import*`.
//! - `"prefix": true`: a family of keys, `KEY.NAME`; its functions take the name first (`{param "exposure" 1.0}`).
//! - `"on"`: `"cue"` (the default), `"heading"`, or `"both"`. A heading key has no functions.
//! - `"levels"` (heading keys): the heading levels it is for; every level when left out.
//! - `"default"` (heading keys): a JSON value of the key's type, given to a heading of those levels without the key.

use lamlet::embed::{Data, Engine, SourceFile, TypeData};
use lamlet_doc::ir::Attrs;
use serde_json::Value as Json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Float,
    Int,
    Bool,
    Vector,
    Text { options: Vec<String>, closed: bool },
    Names { options: Vec<String>, closed: bool },
    /// A static Lamlet type expression, and the packages it needs.
    Type { source: String, imports: Vec<String> },
    Any,
}

/// Where a key may be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum On {
    Cue,
    Heading,
    Both,
}

impl On {
    pub fn cue(self) -> bool {
        matches!(self, On::Cue | On::Both)
    }
    pub fn heading(self) -> bool {
        matches!(self, On::Heading | On::Both)
    }
}

/// One key a cue or heading may carry.
#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    /// The name the functions are named after.
    pub name: String,
    /// The attribute key written in cues (the name, unless given).
    pub key: String,
    pub kind: Kind,
    /// A family of keys, `KEY.NAME`.
    pub prefix: bool,
    pub on: On,
    /// For a heading key, the levels it is for; empty for every level.
    pub levels: Vec<u8>,
    /// For a heading key, the value a heading of its levels gets without it (JSON, read by the key's type).
    pub default: Option<Json>,
}

impl Key {
    /// Whether the attribute key `k` is this key (or one of its family).
    fn matches(&self, k: &str) -> bool {
        if self.prefix {
            k.strip_prefix(self.key.as_str()).and_then(|r| r.strip_prefix('.')).is_some_and(|r| !r.is_empty())
        } else {
            k == self.key
        }
    }

    fn for_level(&self, level: u8) -> bool {
        self.on.heading() && (self.levels.is_empty() || self.levels.contains(&level))
    }

    fn options(&self) -> Option<&[String]> {
        match &self.kind {
            Kind::Text { options, closed: true } | Kind::Names { options, closed: true } => Some(options),
            _ => None,
        }
    }
}

fn strings(v: &Json) -> Option<Vec<String>> {
    v.as_array()?.iter().map(|x| x.as_str().map(String::from)).collect()
}

/// Keys from their JSON description. Fields this crate does not know are ignored, so a host may keep its own.
pub fn keys_from_json(source: &str) -> Result<Vec<Key>, String> {
    let json: Json = serde_json::from_str(source).map_err(|e| format!("the key list is not JSON: {e}"))?;
    let items = json.as_array().ok_or("the key list is a JSON array")?;
    let keys = items
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
                Some(o) => strings(o).ok_or_else(|| at("\"options\" is a list of strings"))?,
            };
            let flag = |f: &str| match p.get(f) {
                None => Ok(false),
                Some(b) => b.as_bool().ok_or_else(|| at(&format!("\"{f}\" is true or false"))),
            };
            let closed = flag("closed")?;
            let prefix = flag("prefix")?;
            let kind = match p.get("kind").and_then(|k| k.as_str()) {
                Some("float") => Kind::Float,
                Some("int") => Kind::Int,
                Some("bool") => Kind::Bool,
                Some("vector") => Kind::Vector,
                Some("text") => Kind::Text { options: options.clone(), closed },
                Some("names") => Kind::Names { options: options.clone(), closed },
                Some("type") => {
                    let source = p.get("type").and_then(|t| t.as_str()).ok_or_else(|| at("a type key needs a \"type\": a Lamlet type expression"))?;
                    let imports = match p.get("imports") {
                        None => vec![],
                        Some(o) => strings(o).ok_or_else(|| at("\"imports\" is a list of package names"))?,
                    };
                    Kind::Type { source: source.to_string(), imports }
                }
                Some("any") => Kind::Any,
                Some(other) => return Err(at(&format!("unknown kind {other:?}; it is float, int, bool, vector, text, names, type, or any"))),
                None => return Err(at("needs a \"kind\" string")),
            };
            let listed = matches!(kind, Kind::Text { .. } | Kind::Names { .. });
            if !options.is_empty() && !listed {
                return Err(at("only a text or names key has \"options\""));
            }
            if closed && (!listed || options.is_empty()) {
                return Err(at("only a text or names key with \"options\" is \"closed\""));
            }
            if prefix && matches!(kind, Kind::Text { .. }) && !options.is_empty() {
                return Err(at("a \"prefix\" key has no \"options\""));
            }
            if (p.get("type").is_some() || p.get("imports").is_some()) && !matches!(kind, Kind::Type { .. }) {
                return Err(at("only a type key has \"type\" and \"imports\""));
            }
            if key.is_empty() || key.chars().any(|c| c.is_whitespace() || c == '"' || c == '=') {
                return Err(at(&format!("{key:?} is not a key: one word without quotes")));
            }
            let on = match p.get("on").map(|o| o.as_str()) {
                None => On::Cue,
                Some(Some("cue")) => On::Cue,
                Some(Some("heading")) => On::Heading,
                Some(Some("both")) => On::Both,
                Some(_) => return Err(at("\"on\" is \"cue\", \"heading\", or \"both\"")),
            };
            let levels = match p.get("levels") {
                None => vec![],
                Some(l) => l
                    .as_array()
                    .and_then(|l| l.iter().map(|x| x.as_u64().filter(|n| (1..=6).contains(n)).map(|n| n as u8)).collect::<Option<Vec<_>>>())
                    .ok_or_else(|| at("\"levels\" is a list of heading levels, 1 to 6"))?,
            };
            let default = p.get("default").cloned();
            if (!levels.is_empty() || default.is_some()) && !on.heading() {
                return Err(at("only a heading key (\"on\": \"heading\" or \"both\") has \"levels\" or a \"default\""));
            }
            if default.is_some() && prefix {
                return Err(at("a \"prefix\" key has no \"default\""));
            }
            Ok(Key { name: name.to_string(), key: key.to_string(), kind, prefix, on, levels, default })
        })
        .collect::<Result<Vec<_>, String>>()?;
    for (i, a) in keys.iter().enumerate() {
        for b in &keys[..i] {
            let levels = a.levels.is_empty() || b.levels.is_empty() || a.levels.iter().any(|l| b.levels.contains(l));
            let overlap = (a.on.cue() && b.on.cue()) || (a.on.heading() && b.on.heading() && levels);
            if a.key == b.key && a.prefix == b.prefix && overlap {
                return Err(format!("the key `{}` is listed twice", a.key));
            }
        }
    }
    Ok(keys)
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

/// The packages the keys' types import, once each, in order.
fn imports(keys: &[Key]) -> Vec<&str> {
    let mut seen = BTreeSet::new();
    keys.iter()
        .flat_map(|k| match &k.kind {
            Kind::Type { imports, .. } => imports.iter().map(String::as_str).collect(),
            _ => vec![],
        })
        .filter(|i| seen.insert(*i))
        .collect()
}

/// The Lamlet source of a vocabulary package: functions for the cue keys. Fails when two functions would have the
/// same name.
pub fn package(title: &str, keys: &[Key]) -> Result<String, String> {
    let mut out = format!(
        "{}\n\nGenerated by `lamlet-present vocab`; do not edit. Each function gives a cue attribute (`content`'s `Attr`).\n\n\
         $[Import* std]$[Import* content]",
        title.replace('$', "$$")
    );
    for i in imports(keys) {
        out.push_str(&format!("$[Import* {i}]"));
    }
    out.push('\n');
    let mut names = BTreeSet::new();
    // A definition `[# {name params} body]`, or `[# name body]` without parameters.
    let mut define = |out: &mut String, name: String, doc: String, params: &[&str], body: String| -> Result<(), String> {
        if !names.insert(name.clone()) {
            return Err(format!("two functions would be named `{name}`"));
        }
        let line = if params.is_empty() { format!("$[# {name} {body}]") } else { format!("$[# {{{name} {}}} {body}]", params.join(" ")) };
        out.push_str(&format!("\n{}\n{line}\n", doc.replace('$', "$$")));
        Ok(())
    };
    for k in keys.iter().filter(|k| k.on.cue()) {
        let base = camel(&[&k.name])?;
        // The attribute key, and the parameters before the value: a family's functions take the name first.
        let (key, shown, lead): (String, String, &[&str]) = if k.prefix {
            (format!("{{++ {} name}}", string(&format!("{}.", k.key))), format!("`{}.NAME`", k.key), &["name"])
        } else {
            (string(&k.key), format!("`{}`", k.key), &[])
        };
        let params = |more: &[&'static str]| -> Vec<&str> { lead.iter().copied().chain(more.iter().copied()).collect() };
        let one = |x: &str, ty: &str| format!("{{attr {key} {{: {x} {ty}}}}}");
        match &k.kind {
            Kind::Float => {
                define(&mut out, base.clone(), format!("{shown}, a number."), &params(&["x"]), one("x", "Float"))?;
                define(
                    &mut out,
                    format!("{base}Range"),
                    format!("{shown} from `a` to `b` over a cue's words."),
                    &params(&["a", "b"]),
                    format!("{{attr {key} {{. from {{: a Float}} to {{: b Float}}}}}}"),
                )?;
            }
            Kind::Int => define(&mut out, base, format!("{shown}, an integer."), &params(&["n"]), one("n", "Int"))?,
            Kind::Bool => define(&mut out, base, format!("{shown}, on or off."), &params(&["b"]), one("b", "Bool"))?,
            Kind::Vector => define(&mut out, base, format!("{shown}, a vector of numbers."), &params(&["xs"]), one("xs", "{List Float}"))?,
            Kind::Names { options, closed } => {
                let doc = if *closed { format!("{shown}, a list of names, each one of: {}.", options.join(", ")) } else { format!("{shown}, a list of names.") };
                define(&mut out, base, doc, &params(&["xs"]), one("xs", "{List String}"))?
            }
            Kind::Text { options, closed } => {
                if !closed {
                    define(&mut out, base.clone(), format!("{shown}, a text value."), &params(&["s"]), one("s", "String"))?;
                }
                for o in options {
                    define(&mut out, camel(&[&base, o])?, format!("{shown} set to `{o}`."), &[], format!("[% {{attr {key} {}}}]", string(o)))?;
                }
            }
            Kind::Type { source, .. } => {
                define(&mut out, base, format!("{shown}, of type `{source}`."), &params(&["x"]), one("x", &format!("[% {source}]")))?
            }
            Kind::Any => {}
        }
    }
    Ok(out)
}

/// Attribute keys every presentation may write: `present`'s own.
const CUE_RESERVED: &[&str] = &["curve", "seconds"];
const HEADING_RESERVED: &[&str] = &["hold", "class"];

/// A key list read for checking: each key with its type and its default.
#[derive(Clone, Debug)]
pub struct Schema {
    keys: Vec<(Key, Option<TypeData>, Option<Data>)>,
}

impl Schema {
    /// The schema of `keys`. A type key's type is evaluated with std, content, present, and `packages` available;
    /// defaults are read by their key's type.
    pub fn new(keys: &[Key], packages: &[(String, String)]) -> Result<Schema, String> {
        // Type keys' types, as the static bindings of a probe document.
        let mut probe = String::from("$[Import* std]$[Import* content]$[Import* present]");
        for i in imports(keys) {
            probe.push_str(&format!("$[Import* {i}]"));
        }
        probe.push('\n');
        for (i, k) in keys.iter().enumerate() {
            if let Kind::Type { source, .. } = &k.kind {
                probe.push_str(&format!("$[# _key{i} {{: [% {source}] Type}}]\n"));
            }
        }
        let mut engine = Engine::new();
        engine.set_resolver(crate::resolver(packages));
        let doc = engine.check(SourceFile::new("<key types>", probe)).map_err(|e| format!("a key's type: {}", e.render().trim_end()))?;
        let mut out = Vec::new();
        for (i, k) in keys.iter().enumerate() {
            let ty = match &k.kind {
                Kind::Float => Some(TypeData::Float),
                Kind::Int => Some(TypeData::Int),
                Kind::Bool => Some(TypeData::Bool),
                Kind::Vector => Some(TypeData::List(Box::new(TypeData::Float))),
                Kind::Text { .. } => Some(TypeData::String),
                Kind::Names { .. } => Some(TypeData::List(Box::new(TypeData::String))),
                Kind::Type { source, .. } => {
                    let b = doc.binding(&format!("_key{i}")).map_err(|e| e.render())?;
                    match Data::from_value(&b.value) {
                        Ok(Data::Type(t)) => Some(t),
                        _ => return Err(format!("the key `{}`: `{source}` is not a type", k.key)),
                    }
                }
                Kind::Any => None,
            };
            let default = match &k.default {
                None => None,
                Some(j) => {
                    let d = from_json(j, ty.as_ref().unwrap_or(&TypeData::Data)).map_err(|m| format!("the default of `{}`: {m}", k.key))?;
                    if let Some(m) = refused(k, &d) {
                        return Err(format!("the default of {m}"));
                    }
                    Some(d)
                }
            };
            out.push((k.clone(), ty, default));
        }
        Ok(Schema { keys: out })
    }

    fn has_cue_keys(&self) -> bool {
        self.keys.iter().any(|(k, ..)| k.on.cue())
    }

    fn has_heading_keys(&self) -> bool {
        self.keys.iter().any(|(k, ..)| k.on.heading())
    }

    /// Every problem with a cue's attributes: keys no cue key matches, and values of the wrong type or not among a
    /// closed key's options. A schema with no cue keys checks nothing.
    pub fn check_cue(&self, attrs: &Attrs) -> Vec<String> {
        if !self.has_cue_keys() {
            return vec![];
        }
        let mut problems = Vec::new();
        for (k, v) in attrs {
            if CUE_RESERVED.contains(&k.as_str()) {
                continue;
            }
            match self.keys.iter().find(|(key, ..)| key.on.cue() && key.matches(k)) {
                None => problems.push(format!("`{k}` is not a cue attribute this host knows")),
                Some((key, ty, _)) => {
                    let fits = match ty {
                        None => true,
                        // A number key also takes a range over a cue's words.
                        Some(TypeData::Float) => conforms(v, &TypeData::Float) || conforms(v, &range()),
                        Some(t) => conforms(v, t),
                    };
                    if !fits {
                        problems.push(format!("`{k}={}` is not {}", json_text(v), described(key, ty.as_ref())));
                    } else if let Some(m) = refused(key, v) {
                        problems.push(m);
                    }
                }
            }
        }
        problems
    }

    /// A heading's attributes read by their keys: each value parsed by its key's kind (prose writes Strings), and
    /// the defaults of the keys it leaves out added in key-list order. A schema with no heading keys returns them
    /// as they are.
    pub fn heading(&self, level: u8, attrs: &Attrs) -> Result<Attrs, Vec<String>> {
        if !self.has_heading_keys() || level == 0 {
            return Ok(attrs.clone());
        }
        let mut out = Vec::new();
        let mut problems = Vec::new();
        for (k, v) in attrs {
            if HEADING_RESERVED.contains(&k.as_str()) {
                out.push((k.clone(), v.clone()));
                continue;
            }
            let Some((key, ty, _)) = self.keys.iter().find(|(key, ..)| key.for_level(level) && key.matches(k)) else {
                problems.push(format!("`{k}` is not an attribute of a level-{level} heading"));
                continue;
            };
            let read = match (v, ty) {
                (_, None) => Ok(v.clone()),
                (Data::String(s), Some(t)) => parse(s, &key.kind, t),
                (v, Some(t)) if conforms(v, t) => Ok(v.clone()),
                (_, Some(_)) => Err(String::new()),
            };
            match read {
                Ok(d) => match refused(key, &d) {
                    Some(m) => problems.push(m),
                    None => out.push((k.clone(), d)),
                },
                Err(m) => {
                    let why = if m.is_empty() { String::new() } else { format!(" ({m})") };
                    problems.push(format!("`{k}={}` is not {}{why}", heading_text(v), described(key, ty.as_ref())));
                }
            }
        }
        for (key, _, default) in &self.keys {
            if let Some(d) = default {
                if key.for_level(level) && !attrs.iter().any(|(k, _)| key.matches(k)) {
                    out.push((key.key.clone(), d.clone()));
                }
            }
        }
        if problems.is_empty() {
            Ok(out)
        } else {
            Err(problems)
        }
    }
}

/// A Float range over a cue's words, `{. from a to b}`.
fn range() -> TypeData {
    TypeData::Record(vec![("from".into(), TypeData::Float), ("to".into(), TypeData::Float)])
}

/// Why a closed key refuses `v`, if it does: a name (or names, in a list) not among its options.
fn refused(key: &Key, v: &Data) -> Option<String> {
    let options = key.options()?;
    let named = |s: &str| options.iter().any(|o| o == s);
    let bad: Vec<&str> = match v {
        Data::String(s) if !named(s) => vec![s],
        Data::List(xs) => xs.iter().filter_map(|x| if let Data::String(s) = x { (!named(s)).then_some(s.as_str()) } else { None }).collect(),
        _ => vec![],
    };
    if bad.is_empty() {
        return None;
    }
    let names = bad.iter().map(|b| format!("`{b}`")).collect::<Vec<_>>().join(", ");
    let verb = if bad.len() == 1 { "is" } else { "are" };
    Some(format!("`{}`: {names} {verb} not one of {}", key.key, options.join(", ")))
}

/// What a key's values are, for a message.
fn described(key: &Key, ty: Option<&TypeData>) -> String {
    match (&key.kind, ty) {
        (Kind::Float, _) => "a number (a Float, or a range `{. from a to b}`)".into(),
        (Kind::Int, _) => "an integer".into(),
        (Kind::Bool, _) => "true or false".into(),
        (Kind::Vector, _) => "a list of numbers".into(),
        (Kind::Text { .. }, _) => "a String".into(),
        (Kind::Names { .. }, _) => "a list of names".into(),
        (Kind::Type { source, .. }, Some(t)) => format!("a value of type `{}` (`{source}`)", type_text(t)),
        _ => "a value".into(),
    }
}

fn json_text(d: &Data) -> String {
    lamlet_doc::backend::json::data(d)
}

fn heading_text(d: &Data) -> String {
    match d {
        Data::String(s) => s.clone(),
        d => json_text(d),
    }
}

/// A type as Lamlet writes it, near enough for a message.
fn type_text(t: &TypeData) -> String {
    let fields = |fs: &[(String, TypeData)]| fs.iter().map(|(l, t)| format!("{l}: {}", type_text(t))).collect::<Vec<_>>().join(", ");
    let list = |ts: &[TypeData]| ts.iter().map(type_text).collect::<Vec<_>>().join(", ");
    match t {
        TypeData::Unit => "Unit".into(),
        TypeData::Bool => "Bool".into(),
        TypeData::Int => "Int".into(),
        TypeData::Float => "Float".into(),
        TypeData::String => "String".into(),
        TypeData::Type => "Type".into(),
        TypeData::Data => "Data".into(),
        TypeData::List(t) => format!("List({})", type_text(t)),
        TypeData::Func(ps, r) => format!("Fn([{}], {})", list(ps), type_text(r)),
        TypeData::Record(fs) => format!("{{{}}}", fields(fs)),
        TypeData::Variant(fs) => format!("<{}>", fields(fs)),
        TypeData::Opaque(id) => format!("Native({id:?})"),
        TypeData::Recursive { name, args, .. } if args.is_empty() => name.clone(),
        TypeData::Recursive { name, args, .. } => format!("{name}({})", list(args)),
        TypeData::Bound(i) => format!("#{i}"),
    }
}

/// `t` with its bound indices replaced by `env` (0 is the recursive type itself, `i` its `i`-th argument). A nested
/// recursive type's body has its own indices, so only its arguments are substituted.
fn substitute(t: &TypeData, env: &[TypeData]) -> TypeData {
    let s = |t: &TypeData| substitute(t, env);
    let fields = |fs: &[(String, TypeData)]| fs.iter().map(|(l, t)| (l.clone(), s(t))).collect();
    match t {
        TypeData::Bound(i) => env.get(*i as usize).cloned().unwrap_or_else(|| t.clone()),
        TypeData::List(x) => TypeData::List(Box::new(s(x))),
        TypeData::Func(ps, r) => TypeData::Func(ps.iter().map(s).collect(), Box::new(s(r))),
        TypeData::Record(fs) => TypeData::Record(fields(fs)),
        TypeData::Variant(fs) => TypeData::Variant(fields(fs)),
        TypeData::Recursive { name, params, body, args } => {
            TypeData::Recursive { name: name.clone(), params: params.clone(), body: body.clone(), args: args.iter().map(s).collect() }
        }
        t => t.clone(),
    }
}

/// One unfolding of a recursive type.
fn unfold(t: &TypeData) -> TypeData {
    match t {
        TypeData::Recursive { body, args, .. } => {
            let env: Vec<TypeData> = std::iter::once(t.clone()).chain(args.iter().cloned()).collect();
            substitute(body, &env)
        }
        t => t.clone(),
    }
}

/// Whether a value has a type.
pub fn conforms(d: &Data, t: &TypeData) -> bool {
    match (t, d) {
        (TypeData::Data, _) => true,
        (TypeData::Unit, Data::Unit) | (TypeData::Bool, Data::Bool(_)) | (TypeData::Int, Data::Int(_)) => true,
        (TypeData::Float, Data::Float(_)) | (TypeData::String, Data::String(_)) | (TypeData::Type, Data::Type(_)) => true,
        (TypeData::List(e), Data::List(xs)) => xs.iter().all(|x| conforms(x, e)),
        (TypeData::Record(fs), Data::Record(r)) => fs.len() == r.len() && fs.iter().all(|(l, t)| r.get(l).is_some_and(|x| conforms(x, t))),
        (TypeData::Variant(alts), Data::Variant(l, x)) => alts.iter().any(|(a, t)| a == l && conforms(x, t)),
        (TypeData::Recursive { .. }, d) => conforms(d, &unfold(t)),
        _ => false,
    }
}

/// A JSON value as data of a type, in the JSON lamlet-doc's IR writes for data (a variant is
/// `{"tag": …, "value": …}`, Unit is `null`).
pub fn from_json(j: &Json, t: &TypeData) -> Result<Data, String> {
    let wrong = || Err(format!("{j} is not a {}", type_text(t)));
    Ok(match (t, j) {
        (TypeData::Data, j) => any_json(j),
        (TypeData::Unit, Json::Null) => Data::Unit,
        (TypeData::Bool, Json::Bool(b)) => Data::Bool(*b),
        (TypeData::Int, Json::Number(n)) => match n.as_i64() {
            Some(n) => Data::Int(n),
            None => return wrong(),
        },
        (TypeData::Float, Json::Number(n)) => match n.as_f64() {
            Some(x) if x.is_finite() => Data::Float(x),
            _ => return wrong(),
        },
        (TypeData::String, Json::String(s)) => Data::String(s.clone()),
        (TypeData::List(e), Json::Array(xs)) => Data::List(xs.iter().map(|x| from_json(x, e)).collect::<Result<_, _>>()?),
        (TypeData::Record(fs), Json::Object(o)) => {
            if o.len() != fs.len() || !fs.iter().all(|(l, _)| o.contains_key(l)) {
                return Err(format!("{j} does not have exactly the fields {}", fs.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>().join(", ")));
            }
            Data::Record(fs.iter().map(|(l, t)| Ok((l.clone(), from_json(&o[l], t)?))).collect::<Result<BTreeMap<_, _>, String>>()?)
        }
        (TypeData::Variant(alts), Json::Object(o)) => {
            let tag = o.get("tag").and_then(|t| t.as_str());
            match alts.iter().find(|(a, _)| Some(a.as_str()) == tag) {
                Some((a, t)) if o.keys().all(|k| k == "tag" || k == "value") => {
                    Data::Variant(a.clone(), Box::new(from_json(o.get("value").unwrap_or(&Json::Null), t)?))
                }
                _ => {
                    let alts = alts.iter().map(|(a, _)| a.as_str()).collect::<Vec<_>>().join(", ");
                    return Err(format!("{j} is not one of {alts}, written {{\"tag\": …, \"value\": …}}"));
                }
            }
        }
        (TypeData::Recursive { .. }, j) => from_json(j, &unfold(t))?,
        _ => return wrong(),
    })
}

/// JSON as data of no particular type: a whole number is an Int.
fn any_json(j: &Json) -> Data {
    match j {
        Json::Null => Data::Unit,
        Json::Bool(b) => Data::Bool(*b),
        Json::Number(n) => n.as_i64().map(Data::Int).unwrap_or_else(|| Data::Float(n.as_f64().unwrap_or(0.0))),
        Json::String(s) => Data::String(s.clone()),
        Json::Array(xs) => Data::List(xs.iter().map(any_json).collect()),
        Json::Object(o) => Data::Record(o.iter().map(|(k, v)| (k.clone(), any_json(v))).collect()),
    }
}

/// A heading attribute's text read by its key's kind: numbers, `true`/`false`, comma-separated lists, and a type
/// key's value as JSON. An empty error means only that the text is not of the kind.
fn parse(s: &str, kind: &Kind, t: &TypeData) -> Result<Data, String> {
    let number = |x: &str| x.trim().parse::<f64>().ok().filter(|x| x.is_finite());
    let list = |s: &str| -> Vec<String> { s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect() };
    let none = || Err(String::new());
    match kind {
        Kind::Float => number(s).map(Data::Float).map_or_else(none, Ok),
        Kind::Int => s.trim().parse::<i64>().map(Data::Int).or_else(|_| none()),
        Kind::Bool => match s.trim() {
            "true" => Ok(Data::Bool(true)),
            "false" => Ok(Data::Bool(false)),
            _ => none(),
        },
        Kind::Vector => list(s).iter().map(|x| number(x).map(Data::Float)).collect::<Option<Vec<_>>>().map(Data::List).map_or_else(none, Ok),
        Kind::Text { .. } | Kind::Any => Ok(Data::String(s.to_string())),
        Kind::Names { .. } => Ok(Data::List(list(s).into_iter().map(Data::String).collect())),
        Kind::Type { .. } => {
            let j: Json = serde_json::from_str(s).map_err(|_| "a type key's heading value is JSON".to_string())?;
            from_json(&j, t)
        }
    }
}
