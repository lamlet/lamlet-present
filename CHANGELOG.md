# Changelog

All notable changes to lamlet-present are listed here. Versions follow [Semantic Versioning](https://semver.org);
while the version is `0.x`, a minor release may break the API.

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
