# lamlet-present

A [Lamlet](https://github.com/OppositeNor/lamlet) extension for narrated presentations: `lamlet present ...`. You write the talk
as text with headings and paragraphs, and mark where things happen between the words. lamlet-present works out
when every section, sentence, and cue happens, from how long the speech actually takes. A host (a slide viewer,
a video pipeline, a game engine) reads that schedule and does the things.

```text
$[Import* std]$[Import* content]$[Import* present]
$[# rise [% {tween 0.0 6.0 1.5 smooth}]]
## One light {#light kind=3d}

${cue "scene" {' {attr "show" "room"}}}This room is lit by ${animateText "light-up" "rise" {'} "one point light"}.
```

The example deck is [`examples/lighting.lam`](examples/lighting.lam).

## Install

```sh
cargo install --git https://github.com/OppositeNor/lamlet-present
# or, from a checkout:
cargo test
cargo install --path .
```

This installs the program `lamlet-present`. Lamlet runs it for `lamlet present ...` (Lamlet's
`docs/documents.md`, "Extensions"); `lamlet-present ...` works too. It carries its Lamlet package, `present`,
inside: `lamlet present source > present.lam` writes it out for other tools, such as
`lamlet build deck.lam --prose commonmark --package present=present.lam`.

## How it works

In compiler terms:

| Stage | Where | What happens |
|---|---|---|
| Frontend | `lamlet` | The document is parsed, type-checked, and its regions evaluated |
| Lowering | `lamlet-doc` | Prose is parsed (CommonMark) into the document IR; each sentence is cut into *pieces* at its cues |
| Deck | `lamlet_present::Deck` | Headings become sections, paragraphs become sentences |
| Voice | the host | Each sentence is synthesized, and the host reports when each piece started (`Narration`) |
| Schedule | `lamlet_present::Schedule` | Every section, sentence, and cue gets its time; animations are sampled per frame |
| Backends | this crate | Schedule JSON, script JSON (what to synthesize), HTML slides |

Lamlet code never plays sound, draws, or waits. The document only describes content and cues, and curves are
pure functions of time. So the same document and the same measurements always give the same schedule, frame
for frame, which makes seeking, replay, and offline rendering safe.

## Writing a presentation

A presentation is an ordinary Lamlet document whose prose is CommonMark:

- **Headings are sections**, such as a slide, a scene, or a chapter. Attributes go in braces:
  `## Room {#room kind=3d hold=2}`. The id names the section; `hold=SECONDS` keeps it on screen after its last
  sentence; any other attribute is passed through to the host.
- **Each paragraph (or list item) is one sentence** to speak. Tables, code blocks, and images are shown but not
  spoken.
- **Cues** come from the `present` package and mark points or stretches of words:

| Function | Meaning |
|---|---|
| `cue name attrs` | Something happens when the next word is spoken |
| `during name attrs content`, `duringText name attrs s` | Something lasts from the first word of the content to its last |
| `animate name curve attrs content`, `animateText …` | Like `during`, and the curve named `curve` is sampled at every frame |
| `pause seconds` | Silence (from `content`) |
| `say shown spoken` | Shown one way, spoken another: a formula, a code name (from `content`) |

Attributes are key and value Strings: `{attr "show" "room"}`. A **vocabulary** (below) gives typed ones.

**Curves** are functions of the seconds since their cue started, of type `Fn([Float], Float)`. They must be
static top-level bindings, a lambda or a curve built during the check:

```text
$[# rise [% {tween 0.0 6.0 1.5 smooth}]]      ;; from 0 to 6 over 1.5 s, smoothly, then hold 6
$[# sway [% {wave 20.0 8.0 4.0}]]             ;; around 20, by 8, every 4 s
```

Easings: `linear`, `easeIn`, `easeOut`, `easeInOut`, `smooth`. Curves: `tween a b seconds ease`,
`wave center amplitude period`, `steady value`. lamlet-present checks every named curve before any speech is
synthesized: a misspelled name or a curve of the wrong type is an error at the sentence's line.

## The commands

```sh
lamlet present script deck.lam > script.json                  # 1. what to synthesize
#                                                               2. the host's voice measures it → narration.json
lamlet present schedule deck.lam --narration narration.json --fps 30 > schedule.json   # 3. the times
lamlet present slides deck.lam > deck.html                    # an HTML slide show, narration as notes (press n)
lamlet present narration deck.lam > narration.json            # an estimated narration: a template, or for drafts
lamlet present vocab lights keys.json > lights.lam            # a typed cue vocabulary (below)
lamlet present source > present.lam                           # the `present` package, for other tools
```

Without `--narration`, `schedule` uses an estimated voice: it produces no audio, but gives a usable draft.

### `script.json`: what to synthesize

```json
{"lamletScript": 1, "title": "A point light",
 "sentences": [{"index": 0, "section": "light", "spoken": "This room is lit by one point light.",
                "pieces": ["", "This room is lit by", "one point light", "."], "shown": "…"}]}
```

A sentence is cut into pieces at its cues. The host synthesizes `spoken` and reports when each piece starts.
Word counts depend on how a voice splits words, so mapping pieces to words is the host's job.

### `narration.json`: what the voice measured

```json
{"lamletNarration": 1,
 "sentences": [{"spoken": "This room is lit by one point light.", "seconds": 2.32, "pieces": [0.0, 0.0, 1.23, 2.18]}]}
```

One entry per sentence: its duration without pauses, and each piece's start in seconds from the sentence's
start. If `spoken` no longer matches the document, the measurement is stale and `schedule` refuses it.

### `schedule.json`: when everything happens

```json
{"lamletSchedule": 1, "title": "A point light", "seconds": 14.9, "fps": 30,
 "sections":  [{"id": "light", "title": "One light", "level": 2, "path": ["title", "light"], "attrs": {"kind": "3d"},
                "start": 0.0, "startFrame": 0, "end": 2.7, "endFrame": 81}],
 "sentences": [{"section": 1, "start": 0.0, "end": 2.32, "spoken": "…", "shown": "…", "pieces": [0.0, 0.0, 1.23, 2.18]}],
 "cues":      [{"name": "light-up", "attrs": {"curve": "rise"}, "section": 1, "sentence": 0,
                "start": 1.23, "end": 2.18, "samples": [{"frame": 37, "t": 0.008, "value": 0.00001}, …]}]}
```

Times are seconds from the start; with `--fps`, each also has a frame. A host maps cue names and attributes to
what it does: show a scene, set a parameter, play a sampled animation.

## Typed cue vocabularies

A host that knows its keys describes them once:

```json
[{"name": "intensity", "kind": "float"},
 {"name": "camera.yaw", "kind": "float"},
 {"name": "mode", "kind": "text", "options": ["albedo", "light-only"]},
 {"name": "label", "kind": "text", "key": "ui.label"}]
```

`lamlet present vocab lights keys.json` writes a package with one typed function per key: `{intensity 0.5}`,
`{intensityRange 0.0 1.0}`, `{cameraYaw 24.0}`, `{mode "albedo"}`, `modeLightOnly`, `{label "hi"}`. A
presentation that imports it writes `${cue "c" {' {intensity 0.5} modeAlbedo}}`. A misspelled key is then an
unknown name, and a wrong value a type error, at the presentation's line. Kinds are `float`, `int`, `bool`,
`vector`, and `text`; `key` sets the attribute key when it should differ from the function's name.

## From Rust

```rust
use lamlet::embed::SourceFile;
use lamlet_present::{Cached, EstimatedVoice, Options, Presentation};

let mut p = Presentation::compile(SourceFile::new("deck.lam", source), &[])?;
let mut voice = Cached::new(EstimatedVoice::default());          // or the host's own Synthesizer
let (narration, schedule) = p.build(&mut voice, &Options { gap: 0.4, fps: Some(30) })?;
```

A real voice implements `Synthesizer`: synthesize the sentence, keep the audio, and return its duration and
the start of each piece. `Cached` synthesizes each distinct sentence once, so rebuilding never synthesizes
again.

## Tests

`cargo test` runs `tests/present.rs`: the deck's structure, schedule times from given measurements (pauses,
holds, cues over words, frames), sampled animations, curve errors before synthesis, stale narration,
vocabularies (typed attributes, typos, type errors, name clashes), slides, the example, and the command.

## Developing against a local Lamlet

`Cargo.toml` takes `lamlet` and `lamlet-doc` from GitHub. To build against a local checkout with unpushed
changes, put a patch in `.cargo/config.toml` (ignored by git):

```toml
[patch."https://github.com/OppositeNor/lamlet"]
lamlet = { path = "../../lamlet/crates/lamlet" }
lamlet-doc = { path = "../../lamlet/crates/lamlet-doc" }
```

## License

MIT. See [LICENSE](LICENSE).
