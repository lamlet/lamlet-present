# Changelog

All notable changes to lamlet-present are listed here. Versions follow [Semantic Versioning](https://semver.org);
while the version is `0.x`, a minor release may break the API.

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
