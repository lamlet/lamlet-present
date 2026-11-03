//! Speech: what the host's voice measured for each sentence.
//!
//! The host synthesizes each sentence with its own text-to-speech engine and reports, per sentence, how long it
//! took and when each piece started. A piece is the spoken text between two cues ([`Utterance::pieces`]), so
//! the host maps pieces to words with its own tokenizer: word counts depend on how the voice splits words.

use crate::speech::Utterance;
use std::collections::HashMap;

/// What a voice measured for one sentence.
#[derive(Clone, Debug, PartialEq)]
pub struct Narration {
    /// The sentence's duration in seconds, without pauses (the schedule adds those).
    pub seconds: f64,
    /// The start of each piece, in seconds from the sentence's start; one per piece of the utterance. An empty
    /// piece starts where the next one does.
    pub pieces: Vec<f64>,
}

impl Narration {
    /// Check that the narration fits its sentence: one time per piece, in order, within the duration.
    pub fn check(&self, u: &Utterance) -> Result<(), String> {
        if !(self.seconds.is_finite() && self.seconds >= 0.0) {
            return Err(format!("a duration of {} seconds", self.seconds));
        }
        if self.pieces.len() != u.pieces.len() {
            return Err(format!("{} piece times for {} pieces", self.pieces.len(), u.pieces.len()));
        }
        let mut last = 0.0;
        for &t in &self.pieces {
            if !(t.is_finite() && t >= last && t <= self.seconds + 1e-9) {
                return Err(format!("piece times must rise from 0 to the duration ({} seconds), not {:?}", self.seconds, self.pieces));
            }
            last = t;
        }
        Ok(())
    }
}

/// A speech engine: synthesizes a sentence, keeps the audio, and reports its timing. Lamlet never calls it;
/// [`crate::Presentation::build`] does, once per sentence.
pub trait Synthesizer {
    fn synthesize(&mut self, sentence: &Utterance) -> Result<Narration, String>;
}

/// A deterministic stand-in for a real voice: each word takes time in proportion to its length. It produces no
/// audio, so presentations can be scheduled and tested offline.
#[derive(Clone, Debug)]
pub struct EstimatedVoice {
    pub seconds_per_char: f64,
    pub seconds_per_word: f64,
}

impl Default for EstimatedVoice {
    fn default() -> Self {
        EstimatedVoice { seconds_per_char: 0.055, seconds_per_word: 0.08 }
    }
}

impl Synthesizer for EstimatedVoice {
    fn synthesize(&mut self, u: &Utterance) -> Result<Narration, String> {
        let mut t = 0.0;
        let mut pieces = Vec::with_capacity(u.pieces.len());
        for p in &u.pieces {
            pieces.push(t);
            for w in p.split_whitespace() {
                t += self.seconds_per_word + self.seconds_per_char * w.chars().count() as f64;
            }
        }
        Ok(Narration { seconds: t, pieces })
    }
}

/// Synthesizes each distinct sentence at most once, so rebuilding a presentation never repeats synthesis.
pub struct Cached<S> {
    pub inner: S,
    cache: HashMap<Vec<String>, Narration>,
    /// How many sentences the inner voice synthesized.
    pub syntheses: usize,
}

impl<S: Synthesizer> Cached<S> {
    pub fn new(inner: S) -> Self {
        Cached { inner, cache: HashMap::new(), syntheses: 0 }
    }
}

impl<S: Synthesizer> Synthesizer for Cached<S> {
    fn synthesize(&mut self, u: &Utterance) -> Result<Narration, String> {
        if let Some(n) = self.cache.get(&u.pieces) {
            return Ok(n.clone());
        }
        self.syntheses += 1;
        let n = self.inner.synthesize(u)?;
        self.cache.insert(u.pieces.clone(), n.clone());
        Ok(n)
    }
}
