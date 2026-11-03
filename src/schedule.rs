//! The schedule: every section, sentence, and cue of a deck with its time, computed from measured speech.
//!
//! Sentences follow each other with a gap ([`Options::gap`]). A pause cue adds silence where it stands. A
//! section starts with its first sentence and lasts until the next section starts; `hold=SECONDS` on its
//! heading adds time at its end (a section with no sentences lasts only its hold). A point cue happens when the
//! piece after it starts; a cue over words lasts from its first word to the end of its last.
//!
//! Times are seconds from the start; with a frame rate, each also has a frame (rounded to the nearest).

use crate::deck::Deck;
use crate::voice::Narration;
use lamlet_doc::backend::speech::AnchorKind;
use lamlet_doc::ir::{attr, Attrs};
use lamlet_doc::Error;
use lamlet::embed::Location;
use serde_json::{json, Map, Value as Json};

/// Schedule settings.
#[derive(Clone, Debug)]
pub struct Options {
    /// Silence after each sentence, in seconds.
    pub gap: f64,
    /// Frames per second, for frame numbers and for sampling curves.
    pub fps: Option<u32>,
}

impl Default for Options {
    fn default() -> Self {
        Options { gap: 0.4, fps: None }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Schedule {
    pub title: String,
    pub seconds: f64,
    pub fps: Option<u32>,
    pub sections: Vec<TimedSection>,
    pub sentences: Vec<TimedSentence>,
    pub cues: Vec<Cue>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TimedSection {
    pub id: String,
    pub title: String,
    pub level: u8,
    pub attrs: Attrs,
    pub path: Vec<String>,
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TimedSentence {
    pub section: usize,
    pub start: f64,
    pub end: f64,
    pub spoken: String,
    pub shown: String,
    /// When each piece starts.
    pub pieces: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub name: String,
    pub attrs: Attrs,
    pub section: usize,
    pub sentence: usize,
    pub start: f64,
    /// The end of a cue over words; `None` for a point cue.
    pub end: Option<f64>,
    /// For an animation, its curve's value at each frame it covers.
    pub samples: Vec<Sample>,
    pub at: Option<Location>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub frame: u64,
    /// Seconds since the cue started.
    pub t: f64,
    pub value: f64,
}

fn error(at: &Option<Location>, message: String) -> Error {
    Error::Doc { message, at: at.clone() }
}

/// A seconds attribute (`hold`, `seconds`), if present.
pub(crate) fn seconds_attr(attrs: &Attrs, key: &str, at: &Option<Location>) -> Result<Option<f64>, Error> {
    match attr(attrs, key) {
        None => Ok(None),
        Some(v) => match v.parse::<f64>() {
            Ok(x) if x.is_finite() && x >= 0.0 => Ok(Some(x)),
            _ => Err(error(at, format!("`{}={}` is not a number of seconds", key, v))),
        },
    }
}

/// The schedule of a deck from one narration per sentence, in order.
pub fn schedule(deck: &Deck, narration: &[Narration], o: &Options) -> Result<Schedule, Error> {
    if narration.len() != deck.sentences.len() {
        return Err(error(&None, format!("{} narrations for {} sentences", narration.len(), deck.sentences.len())));
    }
    let mut t = 0.0;
    // The end of the last sentence or hold: the presentation's length (the gap after the end is not part of it).
    let mut last_end: f64 = 0.0;
    let mut sections = Vec::new();
    let mut sentences = Vec::new();
    let mut cues = Vec::new();
    for (si, s) in deck.sections.iter().enumerate() {
        let start = t;
        for k in s.sentences.clone() {
            let u = &deck.sentences[k].utterance;
            let n = &narration[k];
            n.check(u).map_err(|m| error(&u.at, format!("the narration of this sentence does not fit it: {}", m)))?;
            let local = |p: usize| if p < n.pieces.len() { n.pieces[p] } else { n.seconds };
            // Pauses so far, in the order the cues stand.
            let mut paused = 0.0;
            let mut open: Vec<(String, Attrs, f64)> = Vec::new();
            let mut pieces: Vec<f64> = Vec::with_capacity(n.pieces.len());
            let total_pause: f64 = u.anchors.iter().map(|a| if let AnchorKind::Pause { seconds } = a.kind { seconds } else { 0.0 }).sum();
            for p in 0..n.pieces.len() {
                let before: f64 = u.anchors.iter().filter(|a| a.piece <= p).map(|a| if let AnchorKind::Pause { seconds } = a.kind { seconds } else { 0.0 }).sum();
                pieces.push(t + n.pieces[p] + before);
            }
            for a in &u.anchors {
                let at = t + local(a.piece) + paused;
                match &a.kind {
                    AnchorKind::Pause { seconds } => paused += seconds,
                    AnchorKind::Point { name, attrs } => {
                        cues.push(Cue { name: name.clone(), attrs: attrs.clone(), section: si, sentence: k, start: at, end: None, samples: vec![], at: u.at.clone() })
                    }
                    AnchorKind::Open { name, attrs } => open.push((name.clone(), attrs.clone(), at)),
                    AnchorKind::Close { name } => {
                        let i = open.iter().rposition(|(n, ..)| n == name).ok_or_else(|| error(&u.at, format!("the cue `{}` closes but never opened", name)))?;
                        let (name, attrs, start) = open.remove(i);
                        cues.push(Cue { name, attrs, section: si, sentence: k, start, end: Some(at), samples: vec![], at: u.at.clone() });
                    }
                }
            }
            let end = t + n.seconds + total_pause;
            sentences.push(TimedSentence { section: si, start: t, end, spoken: u.spoken.clone(), shown: u.shown.clone(), pieces });
            last_end = end;
            t = end + o.gap;
        }
        if let Some(hold) = seconds_attr(&s.attrs, "hold", &None).map_err(|e| with_section(e, &s.id))? {
            t += hold;
            last_end = t;
        }
        sections.push(TimedSection { id: s.id.clone(), title: s.title.clone(), level: s.level, attrs: s.attrs.clone(), path: s.path.clone(), start, end: t });
    }
    let seconds = last_end;
    if let Some(last) = sections.last_mut() {
        last.end = seconds;
    }
    cues.sort_by(|a, b| a.start.total_cmp(&b.start));
    Ok(Schedule { title: deck.title.clone(), seconds, fps: o.fps, sections, sentences, cues })
}

fn with_section(e: Error, id: &str) -> Error {
    match e {
        Error::Doc { message, at } => Error::Doc { message: format!("section `{}`: {}", id, message), at },
        e => e,
    }
}

impl Schedule {
    /// A time as a frame, at the schedule's frame rate.
    pub fn frame(&self, t: f64) -> Option<u64> {
        self.fps.map(|f| (t * f as f64).round().max(0.0) as u64)
    }

    /// The schedule as JSON: `{"lamletSchedule": 1, ...}`.
    pub fn to_json(&self) -> Json {
        let time = |o: &mut Map<String, Json>, key: &str, t: f64| {
            o.insert(key.into(), json!(t));
            if let Some(f) = self.frame(t) {
                o.insert(format!("{key}Frame"), json!(f));
            }
        };
        let attrs = |a: &Attrs| Json::Object(a.iter().map(|(k, v)| (k.clone(), json!(v))).collect());
        let sections: Vec<Json> = self
            .sections
            .iter()
            .map(|s| {
                let mut o = Map::new();
                o.insert("id".into(), json!(s.id));
                o.insert("title".into(), json!(s.title));
                o.insert("level".into(), json!(s.level));
                o.insert("path".into(), json!(s.path));
                o.insert("attrs".into(), attrs(&s.attrs));
                time(&mut o, "start", s.start);
                time(&mut o, "end", s.end);
                Json::Object(o)
            })
            .collect();
        let sentences: Vec<Json> = self
            .sentences
            .iter()
            .map(|s| {
                let mut o = Map::new();
                o.insert("section".into(), json!(s.section));
                time(&mut o, "start", s.start);
                time(&mut o, "end", s.end);
                o.insert("spoken".into(), json!(s.spoken));
                o.insert("shown".into(), json!(s.shown));
                o.insert("pieces".into(), json!(s.pieces));
                Json::Object(o)
            })
            .collect();
        let cues: Vec<Json> = self
            .cues
            .iter()
            .map(|c| {
                let mut o = Map::new();
                o.insert("name".into(), json!(c.name));
                o.insert("attrs".into(), attrs(&c.attrs));
                o.insert("section".into(), json!(c.section));
                o.insert("sentence".into(), json!(c.sentence));
                time(&mut o, "start", c.start);
                if let Some(e) = c.end {
                    time(&mut o, "end", e);
                }
                if !c.samples.is_empty() {
                    o.insert("samples".into(), Json::Array(c.samples.iter().map(|s| json!({"frame": s.frame, "t": s.t, "value": s.value})).collect()));
                }
                Json::Object(o)
            })
            .collect();
        let mut o = Map::new();
        o.insert("lamletSchedule".into(), json!(1));
        o.insert("title".into(), json!(self.title));
        time(&mut o, "seconds", self.seconds);
        if let Some(f) = self.fps {
            o.insert("fps".into(), json!(f));
        }
        o.insert("sections".into(), Json::Array(sections));
        o.insert("sentences".into(), Json::Array(sentences));
        o.insert("cues".into(), Json::Array(cues));
        Json::Object(o)
    }
}
