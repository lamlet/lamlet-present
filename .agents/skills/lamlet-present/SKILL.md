---
name: lamlet-present
description: >-
  Write, debug, and extend narrated presentations with lamlet-present, the Lamlet extension (`lamlet present ...`,
  program `lamlet-present`, crate `lamlet_present`): presentation documents (.lam with CommonMark prose, headings as
  sections, paragraphs as sentences), the `present` package's cues and curves, the deck, script, narration,
  schedule, speech, slides, and vocab commands and their JSON, typed cue vocabularies, and the Rust API a host uses
  (Presentation, Deck, Synthesizer, Schedule). Use with the `lamlet` skill, which covers the language itself.
---

# lamlet-present

lamlet-present turns a Lamlet document into a narrated presentation. You write the talk as text; cues mark where
things happen between the words; the host's voice measures the speech; lamlet-present times everything from
those measurements. It never plays, draws, or waits, and it names no host. A host reads its JSON and decides what
cue names and attributes mean.

**Before claiming anything works, run it.** Build with `cargo build`, then use `target/debug/lamlet-present`
(or `lamlet present …` when it is installed). The language itself (syntax, std, types, macros) is the `lamlet`
skill; this one assumes it.

## 1. The pipeline

| Stage | Where | What happens |
|---|---|---|
| Frontend | `lamlet` | The document is parsed, type-checked, evaluated |
| Lowering | `lamlet-doc` | Prose is parsed as CommonMark into the document IR |
| Deck | `lamlet_present::Deck` | Headings become sections, paragraphs (and list items) become sentences |
| Speech | `lamlet_present::speech` | Each sentence is cut into *pieces* at its cues |
| Voice | the host | Each sentence is synthesized; the host reports when each piece started (`Narration`) |
| Schedule | `lamlet_present::Schedule` | Sections, sentences, and cues get times; curves are sampled per frame |

## 2. A presentation document

```text
$[Import* std]$[Import* content]$[Import* present]
$[# rise [% {tween 0.0 6.0 1.5 smooth}]]
# A point light {#title}

## One light {#light kind=3d hold=1}

${cue "scene" {' {attr "show" "room"}}}

This room is lit by ${animateText "light-up" "rise" {'} "one point light"}.

The falloff is ${say {math "1/d^2"} "one over d squared"}${pause 0.5}.
```

- **Prose is always CommonMark** (`lamlet-doc`'s subset): `#` headings with `{#id key=value}` attributes,
  paragraphs, lists, tables, emphasis, code spans, links, images. **No ``` fences**: a fence is an error at its
  line; write `${codeBlock "lang" "code"}`.
- **Sections** are headings. `id` names it (else its text as a slug); `hold=SECONDS` keeps it on after its last
  sentence; every other attribute passes through to the host (`kind=3d`, `sources="a, b"`). Quoted attribute
  values may contain spaces, not quotes or braces.
- **Sentences** are paragraphs and list items. A paragraph of cues only (no words) is a sentence with empty
  `spoken`: hosts use it for keys at a section's start.
- **Tables, code blocks, and images** are shown, never spoken.

## 3. Cues and curves (the `present` package)

| Function | Meaning |
|---|---|
| `{cue name attrs}` | A point between words: happens when the next word is spoken |
| `{during name attrs content}`, `{duringText name attrs s}` | A stretch of words |
| `{animate name curve attrs content}`, `{animateText …}` | Like `during`, and the curve named `curve` is sampled at every frame |
| `{pause seconds}` (from `content`) | Silence |
| `{say shown spoken}` (from `content`) | Shown one way, spoken another; **a formula must be inside `say`** |

- `attrs` is a list of `{attr "key" "value"}` (Strings), or typed functions from a vocabulary (section 6).
- A **curve** is named by String and must be a **static top-level binding** of type `Fn([Float], Float)`, called
  with the seconds since its cue started: `$[# rise [% {tween 0.0 6.0 1.5 smooth}]]` or a lambda
  `$[# {sway t} {sin t}]`. A plain `[# rise {tween …}]` is a *runtime* binding and is refused. Curves are checked
  when the presentation compiles, before any speech.
- Easings: `linear`, `easeIn`, `easeOut`, `easeInOut`, `smooth`. Curves: `tween a b seconds ease`,
  `wave center amplitude period`, `steady value`.
- A point cue that animates needs a `seconds` attribute; better, put the animation over words.

## 4. Commands and their JSON

```sh
lamlet present deck deck.lam            # untimed sections and sentences, each with its IR (for hosts with their own timing)
lamlet present script deck.lam          # what to synthesize
lamlet present narration deck.lam       # an estimated narration, in the form --narration reads
lamlet present schedule deck.lam --narration narration.json --fps 30 [--gap 0.4]
lamlet present speech zh.lam --align en.lam   # every sentence's pieces and cues; check a translation
lamlet present slides deck.lam > deck.html
lamlet present vocab NAME keys.json -o name.lam
lamlet present source > present.lam     # the present package, for `lamlet build --package present=present.lam`
```

Every command takes `--package NAME=PATH` (repeatable) and `-o OUT`.

- **Pieces.** A sentence's spoken text is cut at every cue, span edge, and pause. An anchor's `piece` is the index
  of the piece it stands before. Pieces may be empty (a cue before the first word gives `["", "words…"]`) and
  must stay so: indices are positions. The host maps pieces to words with its own tokenizer.
- **deck** (`{"lamletDeck": 1}`): sections `{id, level, title, path, attrs: [[k, v], …]}`; sentences `{section,
  spoken, pieces, anchors, shown, inlines, at}`. `inlines` is lamlet-doc's IR JSON (the same nodes
  `lamlet build --to ir` writes): `say` nodes keep their `shown` content (a `math` with `tex`, a `code` with
  `text`), so a host can rebuild captions with structure.
- **narration** (`{"lamletNarration": 1}`): per sentence `{spoken, seconds, pieces}`, piece start times in seconds
  from the sentence's start, duration without pauses. A `spoken` that no longer matches is refused as stale.
- **schedule** (`{"lamletSchedule": 1}`): times in seconds, plus `…Frame` fields with `--fps`; cues `{name,
  attrs, start, end?, samples?}`. Animations need `--fps`.
- **speech** (`{"lamletSpeech": 1}`): utterances `{section, spoken, pieces, anchors, shown, at}`. `--align`
  requires the same sections in order, the same paragraph count in each, and no cues in the translation.

## 5. From Rust

```rust
use lamlet::embed::SourceFile;
use lamlet_present::{Cached, EstimatedVoice, Options, Presentation};
let mut p = Presentation::compile(SourceFile::new("deck.lam", source), &[("name".into(), package_source)])?;
let deck_json = p.deck.to_json()?;                                   // untimed
let (narration, schedule) = p.build(&mut Cached::new(EstimatedVoice::default()), &Options { gap: 0.4, fps: Some(30) })?;
```

A host's voice implements `Synthesizer` (`synthesize(&Utterance) -> Narration`). `Presentation::compile_with`
takes an `Engine` the host set up (its resolver must provide `content` and `present`: `lamlet_doc::resolver()`
plus `lamlet_present::PRESENT_SOURCE`). The IR enums are `#[non_exhaustive]`: matches need a catch-all arm.

## 6. Typed cue vocabularies

A host lists its keys; `vocab` writes one typed function per key, so a misspelled key is an unknown name and a
wrong value a type error, at the document's line:

```json
[{"name": "intensity", "kind": "float"}, {"name": "camera.yaw", "kind": "float"},
 {"name": "mode", "kind": "text", "options": ["albedo", "light-only"]}, {"name": "label", "kind": "text", "key": "ui.label"}]
```

gives `{intensity 0.5}`, `{intensityRange 0.0 1.0}` (`"0.0..1.0"`), `{cameraYaw 24.0}`, `{mode "albedo"}`,
`modeAlbedo`, `modeLightOnly`, `{label "hi"}`. Kinds: float, int, bool, vector, text. Names are camel case of the
key; `key` overrides the attribute key. `vocab` checks the package against std, content, present, and every
`--package`, so name clashes show when it is generated. Attribute values are Strings: floats print as Lamlet
writes them (`24.0`), vectors as `1.0,0.5`.

## 7. Pitfalls

| Wrong | Right |
|---|---|
| ```` ``` ```` fences in a presentation | `${codeBlock "rust" "…"}` |
| `${math "x^2"}` in a spoken sentence | `${say {math "x^2"} "x squared"}` |
| `$[# rise {tween …}]` as a curve | `$[# rise [% {tween …}]]` |
| `{animate … "rize" …}` (typo) | caught at compile; the curve's name must match a binding |
| A `$` in narration | `$$` |
| Reading cue times from `deck` | `deck` has no times; use `schedule` (or your own timing from narration) |

## 8. Repository

- `src/lib.rs` (`Presentation`), `deck.rs`, `speech.rs` (utterances, align), `voice.rs` (`Narration`,
  `Synthesizer`, `EstimatedVoice`, `Cached`), `schedule.rs`, `slides.rs`, `vocab.rs`, `present.lam`, `main.rs`.
- Tests: `tests/present.rs`, `tests/speech.rs`; `cargo test`. The CLI tests run the built binary.
- It depends on `lamlet` and `lamlet-doc` by git tag (see `Cargo.toml`); `CHANGELOG.md` per release.
