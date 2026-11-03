//! The presentation model: a document's IR as sections and sentences.
//!
//! Headings start sections. A section holds the blocks under its heading (what a slide shows) and the sentences
//! spoken while it is on screen: one per paragraph or list item, from `lamlet-doc`'s speech backend, with the
//! spoken text cut into pieces at its cues. Blocks before the first heading form an untitled section.

use crate::speech::{self, Utterance};
use lamlet_doc::ir::{Attrs, Block, Doc};
use lamlet_doc::Error;
use std::ops::Range;

/// A presentation: its sections, and the sentences of all sections in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Deck {
    /// The text of the first level-1 heading, or empty.
    pub title: String,
    pub sections: Vec<Section>,
    pub sentences: Vec<Sentence>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    /// The heading's id, else its text in lower case with dashes (`Bounced light` is `bounced-light`); empty for
    /// the untitled section before the first heading.
    pub id: String,
    /// The heading's level, 0 for the untitled section.
    pub level: u8,
    pub title: String,
    /// The heading's attributes: `## Room {#room hold=2}` has `hold` = `2`.
    pub attrs: Attrs,
    /// The ids of the enclosing sections and this one, outermost first.
    pub path: Vec<String>,
    /// The blocks under the heading, up to the next heading.
    pub blocks: Vec<Block>,
    /// The section's sentences, as indices into [`Deck::sentences`].
    pub sentences: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sentence {
    /// The index of its section in [`Deck::sections`].
    pub section: usize,
    pub utterance: Utterance,
}

/// A heading's text as an id: lower case, with runs of other characters as one dash.
pub fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

fn plain(inlines: &[lamlet_doc::Inline]) -> Result<String, Error> {
    let text = lamlet_doc::backend::text::write(&Doc { blocks: vec![Block::Plain(inlines.to_vec())] })?;
    Ok(text.split_whitespace().collect::<Vec<_>>().join(" "))
}

impl Deck {
    /// The deck of a lowered document.
    pub fn from_doc(doc: &Doc) -> Result<Deck, Error> {
        let mut sections: Vec<Section> = Vec::new();
        let mut current = Section { id: String::new(), level: 0, title: String::new(), attrs: vec![], path: vec![], blocks: vec![], sentences: 0..0 };
        let mut open: Vec<(u8, String)> = Vec::new();
        for block in &doc.blocks {
            match block {
                Block::Heading { level, id, attrs, inlines } => {
                    sections.push(std::mem::replace(&mut current, Section { id: String::new(), level: 0, title: String::new(), attrs: vec![], path: vec![], blocks: vec![], sentences: 0..0 }));
                    let title = plain(inlines)?;
                    let id = id.clone().unwrap_or_else(|| slug(&title));
                    open.retain(|(l, _)| l < level);
                    open.push((*level, id.clone()));
                    current = Section { id, level: *level, title, attrs: attrs.clone(), path: open.iter().map(|(_, i)| i.clone()).collect(), blocks: vec![], sentences: 0..0 };
                }
                b => current.blocks.push(b.clone()),
            }
        }
        sections.push(current);
        // The untitled section is kept only when it has something in it.
        if sections[0].blocks.iter().all(|b| matches!(b, Block::Plain(xs) if xs.is_empty())) {
            sections.remove(0);
        }
        let mut sentences = Vec::new();
        for (i, s) in sections.iter_mut().enumerate() {
            let start = sentences.len();
            for mut u in speech::utterances(&Doc { blocks: s.blocks.clone() })? {
                u.section = s.path.clone();
                sentences.push(Sentence { section: i, utterance: u });
            }
            s.sentences = start..sentences.len();
        }
        let title = sections.iter().find(|s| s.level == 1).map(|s| s.title.clone()).unwrap_or_default();
        Ok(Deck { title, sections, sentences })
    }
}

impl Deck {
    /// The deck as JSON: `{"lamletDeck": 1, "title", "sections", "sentences"}`, with no timing. A host that applies
    /// timing rules of its own reads this instead of a schedule.
    ///
    /// - A section is `{id, level, title, path, attrs}`, with `attrs` as `[[key, value], ...]` in source order.
    /// - A sentence is `{section, spoken, pieces, anchors, shown, inlines, at}`: `anchors` as in
    ///   [`crate::speech::to_json`], `inlines` as lamlet-doc's IR JSON (`lamlet build --to ir`), and `at` the
    ///   sentence's `{file, line, column}`.
    pub fn to_json(&self) -> Result<serde_json::Value, Error> {
        use serde_json::{json, Value};
        let pairs = |a: &Attrs| Value::Array(a.iter().map(|(k, v)| json!([k, v])).collect());
        let sections: Vec<Value> =
            self.sections.iter().map(|s| json!({"id": s.id, "level": s.level, "title": s.title, "path": s.path, "attrs": pairs(&s.attrs)})).collect();
        let speech = crate::speech::to_json(&self.sentences.iter().map(|s| s.utterance.clone()).collect::<Vec<_>>());
        let mut sentences = Vec::new();
        for (s, u) in self.sentences.iter().zip(speech["utterances"].as_array().into_iter().flatten()) {
            let ir = lamlet_doc::backend::json::write(&Doc { blocks: vec![Block::Plain(s.utterance.inlines.clone())] })?;
            let ir: Value = serde_json::from_str(&ir).expect("lamlet-doc writes valid JSON");
            let mut o = u.as_object().cloned().unwrap_or_default();
            o.remove("section");
            o.insert("section".into(), json!(s.section));
            o.insert("inlines".into(), ir["blocks"][0]["inlines"].clone());
            sentences.push(Value::Object(o));
        }
        Ok(json!({"lamletDeck": 1, "title": self.title, "sections": sections, "sentences": sentences}))
    }
}
