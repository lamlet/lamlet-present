//! Narrated presentations written as Lamlet documents.
//!
//! A presentation is an ordinary Lamlet document. Its prose is parsed as CommonMark: headings are sections (a
//! slide, a scene, a chapter), each paragraph is a sentence to speak, and cues from the `present` package mark
//! where things happen between the words. The pipeline, in compiler terms:
//!
//! ```text
//! frontend   lamlet         parse, type-check, evaluate the regions
//! lowering   lamlet-doc     the document IR, with speech: sentences cut into pieces at their cues
//! deck       this crate     sections and sentences (Deck)
//! voice      the host       speech synthesized and measured, per sentence (Synthesizer → Narration)
//! schedule   this crate     every section, sentence, and cue with its time, and curves sampled per frame
//! backends   this crate     schedule JSON, script JSON (what to synthesize), HTML slides
//! ```
//!
//! Lamlet code never plays, draws, or waits: the document computes content and cues, and curves are pure
//! functions of time. So the same document and the same measurements give the same schedule, frame for frame.
//! The crate names no host: a host reads the schedule and maps cue names and attributes to what it does.

pub mod deck;
pub mod schedule;
pub mod slides;
pub mod speech;
pub mod vocab;
pub mod voice;

pub use deck::{Deck, Section, Sentence};
pub use lamlet_doc::Error;
pub use schedule::{Cue, Options, Sample, Schedule};
pub use voice::{Cached, EstimatedVoice, Narration, Synthesizer};

use lamlet::embed::{Document, Engine, Exports, Instance, Location, MemoryResolver, SourceFile};
use crate::speech::AnchorKind;
use lamlet_doc::ir::{attr, Attrs};
use serde_json::{json, Value as Json};

/// The `present` package's Lamlet source: cues (`cue`, `during`, `animate`) and curves (`tween`, `wave`, easings).
pub const PRESENT_SOURCE: &str = include_str!("present.lam");

/// A resolver with `std`, `content`, `present`, and the given packages (name, source).
pub fn resolver(packages: &[(String, String)]) -> MemoryResolver {
    let mut r = lamlet_doc::resolver();
    r.add("present", PRESENT_SOURCE, Exports::All);
    for (name, source) in packages {
        r.add(name, source, Exports::All);
    }
    r
}

fn error(at: &Option<Location>, message: impl Into<String>) -> Error {
    Error::Doc { message: message.into(), at: at.clone() }
}

/// An attribute value as JSON data, as lamlet-doc's IR JSON writes it (a String stays a JSON string).
pub(crate) fn data_json(d: &lamlet::embed::Data) -> Json {
    serde_json::from_str(&lamlet_doc::backend::json::data(d)).expect("lamlet-doc writes valid JSON")
}

/// Attributes as a JSON object of key to data.
pub(crate) fn attrs_object(a: &Attrs) -> Json {
    Json::Object(a.iter().map(|(k, v)| (k.clone(), data_json(v))).collect())
}

/// The name of the curve an animation follows (its `curve` attribute), which must be a String.
fn curve_attr<'a>(attrs: &'a Attrs, at: &Option<Location>) -> Result<Option<&'a str>, Error> {
    match attr(attrs, "curve") {
        None => Ok(None),
        Some(lamlet::embed::Data::String(s)) => Ok(Some(s)),
        Some(v) => Err(error(at, format!("`curve={}` is not a String naming a curve", lamlet_doc::backend::json::data(v)))),
    }
}

/// A checked and evaluated presentation, and an instance to sample its curves.
pub struct Presentation {
    pub deck: Deck,
    doc: Document,
    instance: Instance,
    _engine: Engine,
}

impl Presentation {
    /// Check, evaluate, and lower a presentation. Curves its animations name are checked here, before any speech
    /// is synthesized.
    pub fn compile(file: SourceFile, packages: &[(String, String)]) -> Result<Presentation, Error> {
        let mut engine = Engine::new();
        engine.set_resolver(resolver(packages));
        let doc = engine.check(file)?;
        let mut instance = engine.instance(&doc)?;
        let ir = lamlet_doc::evaluate_and_lower(&doc, &mut instance, lamlet_doc::COMMONMARK)?;
        let deck = Deck::from_doc(&ir)?;
        let mut p = Presentation { deck, doc, instance, _engine: engine };
        p.check_curves()?;
        Ok(p)
    }

    fn check_curves(&mut self) -> Result<(), Error> {
        for s in &self.deck.sentences {
            for a in &s.utterance.anchors {
                let (AnchorKind::Open { attrs, .. } | AnchorKind::Point { attrs, .. }) = &a.kind else { continue };
                let at = &s.utterance.at;
                let Some(curve) = curve_attr(attrs, at)? else { continue };
                if matches!(a.kind, AnchorKind::Point { .. }) && attr(attrs, "seconds").is_none() {
                    return Err(error(at, format!("the point cue animating `{}` needs `seconds`, or put the animation over words", curve)));
                }
                sample(&mut self.instance, &self.doc, curve, 0.0, at)?;
            }
        }
        Ok(())
    }

    /// The schedule from one narration per sentence. With a frame rate, every animation's curve is sampled at
    /// each frame it covers.
    pub fn schedule(&mut self, narration: &[Narration], o: &Options) -> Result<Schedule, Error> {
        let mut s = schedule::schedule(&self.deck, narration, o)?;
        for cue in &mut s.cues {
            let Some(curve) = curve_attr(&cue.attrs, &cue.at)?.map(String::from) else { continue };
            let Some(fps) = o.fps else {
                return Err(error(&cue.at, format!("the animation `{}` needs a frame rate to be sampled (`Options::fps`, or `--fps`)", cue.name)));
            };
            let end = match cue.end {
                Some(e) => e,
                None => cue.start + schedule::seconds_attr(&cue.attrs, "seconds", &cue.at)?.unwrap_or(0.0),
            };
            let (f0, f1) = ((cue.start * fps as f64).round() as u64, (end * fps as f64).round() as u64);
            for frame in f0..=f1 {
                let t = (frame as f64 / fps as f64 - cue.start).max(0.0);
                let value = sample(&mut self.instance, &self.doc, &curve, t, &cue.at)?;
                cue.samples.push(Sample { frame, t, value });
            }
        }
        Ok(s)
    }

    /// Synthesize every sentence with the host's voice.
    pub fn synthesize(&self, voice: &mut dyn Synthesizer) -> Result<Vec<Narration>, Error> {
        self.deck
            .sentences
            .iter()
            .map(|s| voice.synthesize(&s.utterance).map_err(|m| error(&s.utterance.at, format!("the voice failed: {}", m))))
            .collect()
    }

    /// Synthesize every sentence with the host's voice, then schedule. Returns the measurements too.
    pub fn build(&mut self, voice: &mut dyn Synthesizer, o: &Options) -> Result<(Vec<Narration>, Schedule), Error> {
        let narration = self.synthesize(voice)?;
        let s = self.schedule(&narration, o)?;
        Ok((narration, s))
    }

    /// What to synthesize, as JSON: `{"lamletScript": 1, "sentences": [{index, section, spoken, pieces, shown}]}`.
    pub fn script_json(&self) -> Json {
        let sentences: Vec<Json> = self
            .deck
            .sentences
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let u = &s.utterance;
                json!({"index": i, "section": self.deck.sections[s.section].id, "spoken": u.spoken, "pieces": u.pieces, "shown": u.shown})
            })
            .collect();
        json!({"lamletScript": 1, "title": self.deck.title, "sentences": sentences})
    }
}

/// A curve's value at `t` seconds: the named top-level binding, called with a Float.
fn sample(instance: &mut Instance, doc: &Document, curve: &str, t: f64, at: &Option<Location>) -> Result<f64, Error> {
    let b = doc.binding(curve).map_err(|_| error(at, format!("there is no static top-level binding `{}` to animate with (write the curve as a lambda, or as `[# {} [% ...]]`)", curve, curve)))?;
    let v: f64 = instance
        .call_typed(&b, (t,))
        .map_err(|e| error(at, format!("the curve `{}` (of type `{}`) is not `Fn([Float], Float)`: {}", curve, b.type_text, e.render().trim_end())))?;
    if !v.is_finite() {
        return Err(error(at, format!("the curve `{}` gave {} at {} seconds", curve, v, t)));
    }
    Ok(v)
}

/// Narration as JSON: `{"lamletNarration": 1, "sentences": [{spoken, seconds, pieces}]}`, one per sentence.
pub fn narration_json(deck: &Deck, narration: &[Narration]) -> Json {
    let sentences: Vec<Json> =
        deck.sentences.iter().zip(narration).map(|(s, n)| json!({"spoken": s.utterance.spoken, "seconds": n.seconds, "pieces": n.pieces})).collect();
    json!({"lamletNarration": 1, "sentences": sentences})
}

/// Narration read from JSON (as [`narration_json`] writes it). A sentence whose `spoken` differs from the deck's
/// was measured for other words, and is an error: synthesize it again.
pub fn narration_from_json(deck: &Deck, json: &Json) -> Result<Vec<Narration>, Error> {
    let none = &None;
    let items = json.get("sentences").and_then(|s| s.as_array()).ok_or_else(|| error(none, "narration JSON has a \"sentences\" list"))?;
    if items.len() != deck.sentences.len() {
        return Err(error(none, format!("the narration has {} sentences but the presentation {}: synthesize it again", items.len(), deck.sentences.len())));
    }
    deck.sentences
        .iter()
        .zip(items)
        .enumerate()
        .map(|(i, (s, n))| {
            let at = &s.utterance.at;
            if let Some(spoken) = n.get("spoken").and_then(|x| x.as_str()) {
                if spoken != s.utterance.spoken {
                    return Err(error(at, format!("narration {} was measured for {:?}, not this sentence: synthesize it again", i, spoken)));
                }
            }
            let seconds = n.get("seconds").and_then(|x| x.as_f64()).ok_or_else(|| error(at, format!("narration {} has no \"seconds\"", i)))?;
            let pieces = n
                .get("pieces")
                .and_then(|x| x.as_array())
                .and_then(|a| a.iter().map(|x| x.as_f64()).collect::<Option<Vec<_>>>())
                .ok_or_else(|| error(at, format!("narration {} has no \"pieces\" list of numbers", i)))?;
            Ok(Narration { seconds, pieces })
        })
        .collect()
}
