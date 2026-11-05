# Changelog

All notable changes to lamlet-present are listed here. Versions follow [Semantic Versioning](https://semver.org);
while the version is `0.x`, a minor release may break the API.

## 0.3.0

Needs Lamlet 0.4.0, as 0.2.0 did. No change to Lamlet itself.

- Changed: a vocabulary's functions write typed data instead of Strings: a Float, an Int, a Bool, a `List Float`
  for a vector, and a range as `{. from a to b}` (it was `"a..b"`). A host that parsed the Strings reads the
  data instead; `lamlet_doc::ir::attr_f64` already reads a Float or an Int.
- `Kind` has new kinds, and `Key` new fields (`prefix`, `on`, `levels`, `default`); `Kind::Text` has `closed`.
- Added: `"closed": true` on a text key with options: only the option constants, no function that takes any
  String, so a wrong option is an unknown name.
- Added: kinds `names` (a `List String`, optionally closed to its options), `type` (any static Lamlet type, with
  `imports`), and `any` (a key the host writes itself, with no function).
- Added: `"prefix": true`, a family of keys `KEY.NAME` whose functions take the name first.
- Added: `vocab::Schema` and `Presentation::apply_keys`, and `--keys KEYS.json` on every command that reads a
  presentation. Every cue attribute, also one written with `{attr …}`, must be a cue key's, with a value of its
  type and, for a closed key, among its options. Heading keys (`"on": "heading"` or `"both"`, `"levels"`,
  `"default"`) make every heading attribute a key of its level, read by its kind into typed data, with defaults
  for the keys a heading leaves out. Every problem is reported.

## 0.2.0

Needs Lamlet 0.4.0 (`lamlet` and `lamlet-doc` at the `v0.4.0` tag). A host must pin the same tag.

- Changed: attribute values are typed data, as in lamlet-doc 0.4. `Section::attrs`, `Cue::attrs`, and an anchor's
  `attrs` are `Vec<(String, lamlet::embed::Data)>` (`lamlet_doc::ir::Attrs`), and a cue can carry any value with
  equality (`{attr "seconds" 1.5}`, `{attr "at" {. x 1.0 y 2.0}}`).
- Changed: the deck, schedule, and speech JSON write each attribute value as JSON data, as lamlet-doc's IR JSON
  does. String values, which every document written for 0.1 has, are written as before.
- `hold` and `seconds` take a Float or an Int, or a String that parses as a number (as before). `curve` must be a
  String; any other value is an error at its sentence.
- The example deck uses the `f"{...}"` template form; Lamlet 0.4 retired `[f "..."]`.

## 0.1.3

- Fixed: `Cargo.lock` records Lamlet 0.3.1. In 0.1.2 it still recorded 0.3.0, so
  `cargo install --locked --git … --tag v0.1.2` and `cargo test --locked` refused to run. Hosts that depend on
  lamlet-present use their own lockfile and were not affected.

## 0.1.2

- Needs Lamlet 0.3.1 (`lamlet` and `lamlet-doc` at the `v0.3.1` tag), which can be vendored: a host that vendors
  its dependencies can now build lamlet-present offline. A host must pin the same Lamlet tag as lamlet-present, or
  its build holds two incompatible copies of Lamlet.

## 0.1.1

- The `lamlet-present` agent skill (`.agents/skills/lamlet-present/SKILL.md`, linked from `.claude/skills`): the
  presentation format, cues and curves, every command and its JSON, the Rust API, vocabularies, and pitfalls.

## 0.1.0

First tagged release. It needs Lamlet 0.3 (`lamlet` and `lamlet-doc` at the `v0.3.0` tag).

- A Lamlet extension: the program `lamlet-present`, run by Lamlet as `lamlet present ...`.
- Presentations are Lamlet documents whose prose is CommonMark: headings are sections, paragraphs are
  sentences, and the `present` package's cues (`cue`, `during`, `animate`) and curves (`tween`, `wave`,
  easings) mark what happens between the words.
- `script`: what to synthesize. `narration`: an estimated narration, the form a host's voice fills in.
- `schedule`: every section, sentence, and cue timed from measured speech, with curves sampled per frame.
- `deck`: the sections and sentences with no timing, each sentence with its content as Lamlet's IR, for a host
  that applies timing rules of its own.
- `speech`: every sentence's spoken and shown text, pieces, and cues; `--align` checks a translation against its
  original.
- `slides`: an HTML slide show with the narration as speaker notes.
- `vocab`: a typed cue vocabulary generated from a host's keys.
- `source`: the `present` package, for other tools.
