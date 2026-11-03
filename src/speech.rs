//! Speech: a document's IR as utterances for a speech host. (This was `lamlet_doc::backend::speech`; speaking
//! documents is lamlet-present's job, so it lives here.)
//!
//! An utterance is one paragraph: a `Paragraph` block, a list item, or, in unparsed prose, the text between
//! blank lines. Headings set the section (the ids of the enclosing headings, or their text when they have none)
//! and are not spoken. Tables, code blocks, and raw blocks are not spoken.
//!
//! Within an utterance, the spoken text is cut into *pieces* at every anchor, span edge, and pause, so a host can
//! time each one to the word that follows it. The backend does not count words: word counts depend on how the
//! voice splits words, so the host maps pieces to words with its own tokenizer.
//!
//! | Node | Spoken as |
//! |---|---|
//! | `Text`, `Code`, `Raw` in [`lamlet_doc::TEXT`] or parsed prose | as written, with runs of whitespace as one space |
//! | `Value` | its written form |
//! | `Say` | its `spoken`; what is shown is `shown` |
//! | `Link`, `Emph`, `Strong` | their content |
//! | `Image` | its alternative text |
//! | `Math` | an error: a formula needs `say` to be spoken |
//! | `Anchor` | a cut, with an anchor `{kind: "point"}` on the next piece |
//! | `Span` | cuts at both ends, with anchors `open` and `close` |
//! | `Pause` | a cut, with an anchor `{kind: "pause", seconds}` |
//!
//! The JSON form is `{"lamletSpeech": 1, "utterances": [...]}`.

use lamlet::embed::Location;
use lamlet_doc::ir::{Attrs, Block, Doc, Inline};
use lamlet_doc::{written_form, Error, COMMONMARK, TEXT};
use serde_json::{json, Map, Value as Json};

fn error(at: &Option<Location>, message: impl Into<String>) -> Error {
    Error::Doc { message: message.into(), at: at.clone() }
}

fn plain(inlines: &[Inline]) -> Result<String, Error> {
    lamlet_doc::backend::text::write(&Doc { blocks: vec![Block::Plain(inlines.to_vec())] })
}

/// One paragraph to speak.
#[derive(Clone, Debug, PartialEq)]
pub struct Utterance {
    /// The ids (or texts) of the enclosing headings, outermost first.
    pub section: Vec<String>,
    /// The spoken text: the non-empty pieces, joined by spaces.
    pub spoken: String,
    /// The spoken text cut at each anchor. Pieces may be empty, so anchor indices stay stable.
    pub pieces: Vec<String>,
    pub anchors: Vec<Anchor>,
    /// What is shown while the utterance is spoken, as plain text.
    pub shown: String,
    /// The sentence's content as the IR has it (text, formulas, code, cues), so a host can draw what is shown
    /// with its structure.
    pub inlines: Vec<Inline>,
    pub at: Option<Location>,
}

/// A point between pieces.
#[derive(Clone, Debug, PartialEq)]
pub struct Anchor {
    /// The index of the piece the anchor stands before (`pieces.len()` for the end).
    pub piece: usize,
    pub kind: AnchorKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AnchorKind {
    Point { name: String, attrs: Attrs },
    Open { name: String, attrs: Attrs },
    Close { name: String },
    Pause { seconds: f64 },
}

/// The document's utterances.
pub fn utterances(doc: &Doc) -> Result<Vec<Utterance>, Error> {
    let mut w = Walker::default();
    w.blocks(&doc.blocks)?;
    w.flush();
    Ok(w.out)
}

/// The utterances as JSON text.
pub fn write(doc: &Doc) -> Result<String, Error> {
    Ok(serde_json::to_string_pretty(&to_json(&utterances(doc)?)).expect("JSON") + "\n")
}

/// Utterances as JSON: `{"lamletSpeech": 1, "utterances": [{section, spoken, pieces, anchors, shown, at}]}`.
pub fn to_json(us: &[Utterance]) -> Json {
    let attrs = crate::attrs_object;
    let items: Vec<Json> = us
        .iter()
        .map(|u| {
            let anchors: Vec<Json> = u
                .anchors
                .iter()
                .map(|a| match &a.kind {
                    AnchorKind::Point { name, attrs: x } => json!({"piece": a.piece, "kind": "point", "name": name, "attrs": attrs(x)}),
                    AnchorKind::Open { name, attrs: x } => json!({"piece": a.piece, "kind": "open", "name": name, "attrs": attrs(x)}),
                    AnchorKind::Close { name } => json!({"piece": a.piece, "kind": "close", "name": name}),
                    AnchorKind::Pause { seconds } => json!({"piece": a.piece, "kind": "pause", "seconds": seconds}),
                })
                .collect();
            let mut o = Map::new();
            o.insert("section".into(), json!(u.section));
            o.insert("spoken".into(), json!(u.spoken));
            o.insert("pieces".into(), json!(u.pieces));
            o.insert("anchors".into(), Json::Array(anchors));
            o.insert("shown".into(), json!(u.shown));
            if let Some(at) = &u.at {
                o.insert("at".into(), json!({"file": at.file, "line": at.line, "column": at.column}));
            }
            Json::Object(o)
        })
        .collect();
    json!({"lamletSpeech": 1, "utterances": items})
}

/// Check that a translation matches its original: the same sections in the same order, the same number of
/// utterances in each, and no anchors or spans (a translation is shown and spoken, but carries no cues).
pub fn align(original: &[Utterance], translation: &[Utterance]) -> Result<(), Error> {
    let groups = |us: &[Utterance]| {
        let mut g: Vec<(Vec<String>, usize, Option<Location>)> = Vec::new();
        for u in us {
            match g.last_mut() {
                Some((s, n, _)) if *s == u.section => *n += 1,
                _ => g.push((u.section.clone(), 1, u.at.clone())),
            }
        }
        g
    };
    if let Some(u) = translation.iter().find(|u| u.anchors.iter().any(|a| !matches!(a.kind, AnchorKind::Pause { .. }))) {
        return Err(error(&u.at, "a translation carries no anchors or spans; its original has the cues"));
    }
    let (a, b) = (groups(original), groups(translation));
    for (i, (o, t)) in a.iter().zip(&b).enumerate() {
        if o.0 != t.0 {
            return Err(error(&t.2, format!("section {} is `{}` here but `{}` in the original", i + 1, t.0.join("/"), o.0.join("/"))));
        }
        if o.1 != t.1 {
            return Err(error(&t.2, format!("section `{}` has {} paragraphs here but {} in the original", t.0.join("/"), t.1, o.1)));
        }
    }
    if a.len() != b.len() {
        let at = b.get(a.len()).map(|g| g.2.clone()).unwrap_or(None);
        return Err(error(&at, format!("the translation has {} sections and the original {}", b.len(), a.len())));
    }
    Ok(())
}

#[derive(Default)]
struct Walker {
    out: Vec<Utterance>,
    /// Headings by level: (level, id or text).
    sections: Vec<(u8, String)>,
    pieces: Vec<String>,
    anchors: Vec<Anchor>,
    shown: String,
    at: Option<Location>,
    /// The current sentence's top-level inlines, and how deep the walk is inside one.
    recorded: Vec<Inline>,
    depth: usize,
}

/// Non-empty pieces joined by spaces, with no space before closing punctuation.
fn join_pieces(pieces: &[String]) -> String {
    let mut out = String::new();
    for p in pieces.iter().filter(|p| !p.is_empty()) {
        if !out.is_empty() && !p.starts_with(['.', ',', ';', ':', '!', '?', ')', ']', '\'', '"']) {
            out.push(' ');
        }
        out.push_str(p);
    }
    out
}

/// Runs of whitespace as one space, and no space before closing punctuation.
fn collapse(s: &str) -> String {
    let mut out = s.split_whitespace().collect::<Vec<_>>().join(" ");
    for p in [".", ",", ";", ":", "!", "?", ")"] {
        out = out.replace(&format!(" {p}"), p);
    }
    out
}

impl Walker {
    fn flush(&mut self) {
        let pieces: Vec<String> = std::mem::take(&mut self.pieces).iter().map(|p| collapse(p)).collect();
        let anchors = std::mem::take(&mut self.anchors);
        let shown = collapse(&std::mem::take(&mut self.shown));
        let at = self.at.take();
        let inlines = std::mem::take(&mut self.recorded);
        let spoken = join_pieces(&pieces);
        if spoken.is_empty() && anchors.is_empty() {
            return;
        }
        let section = self.sections.iter().map(|(_, s)| s.clone()).collect();
        self.out.push(Utterance { section, spoken, pieces, anchors, shown, inlines, at });
    }

    fn piece(&mut self) -> &mut String {
        if self.pieces.is_empty() {
            self.pieces.push(String::new());
        }
        self.pieces.last_mut().unwrap()
    }

    fn say(&mut self, s: &str) {
        self.piece().push_str(s);
    }

    fn cut(&mut self, kind: AnchorKind) {
        if self.pieces.is_empty() {
            self.pieces.push(String::new());
        }
        self.pieces.push(String::new());
        self.anchors.push(Anchor { piece: self.pieces.len() - 1, kind });
    }

    fn blocks(&mut self, bs: &[Block]) -> Result<(), Error> {
        bs.iter().try_for_each(|b| self.block(b))
    }

    fn block(&mut self, b: &Block) -> Result<(), Error> {
        match b {
            Block::Plain(xs) => self.inlines(xs, true)?,
            Block::Paragraph(xs) => {
                self.flush();
                self.inlines(xs, false)?;
                self.flush();
            }
            Block::Heading { level, id, inlines, .. } => {
                self.flush();
                let title = plain(inlines)?;
                self.sections.retain(|(l, _)| l < level);
                self.sections.push((*level, id.clone().unwrap_or_else(|| collapse(&title))));
            }
            Block::List { items, .. } => {
                for item in items {
                    self.flush();
                    self.blocks(item)?;
                    self.flush();
                }
            }
            Block::Div { blocks, .. } => self.blocks(blocks)?,
            // Tables, code blocks, raw markup, and kinds of block this backend does not know are shown, not spoken.
            _ => self.flush(),
        }
        Ok(())
    }

    /// `split`: in unparsed prose, a blank line ends an utterance.
    fn inlines(&mut self, xs: &[Inline], split: bool) -> Result<(), Error> {
        xs.iter().try_for_each(|x| self.inline(x, split))
    }

    /// `record`: a top-level text node of unparsed prose, recorded part by part, since a blank line inside it
    /// ends the sentence.
    fn text(&mut self, text: &str, split: bool, at: &Option<Location>, record: Option<&Inline>) {
        let mut parts = if split { paragraphs(text) } else { vec![text.to_string()] }.into_iter().peekable();
        while let Some(part) = parts.next() {
            if self.at.is_none() && !part.trim().is_empty() {
                self.at = at.clone();
            }
            match record {
                Some(Inline::Raw { format, .. }) => self.recorded.push(Inline::Raw { format: format.clone(), text: part.clone(), at: at.clone() }),
                Some(_) => self.recorded.push(Inline::Text { text: part.clone(), at: at.clone() }),
                None => {}
            }
            self.say(&part);
            self.shown.push_str(&part);
            if parts.peek().is_some() {
                self.flush();
            }
        }
    }

    fn inline(&mut self, x: &Inline, split: bool) -> Result<(), Error> {
        let top = self.depth == 0;
        let split_text = split && matches!(x, Inline::Text { .. } | Inline::Raw { .. });
        if top && !split_text {
            self.recorded.push(x.clone());
        }
        self.depth += 1;
        let r = self.walk_inline(x, split, if top && split_text { Some(x) } else { None });
        self.depth -= 1;
        r
    }

    fn walk_inline(&mut self, x: &Inline, split: bool, record: Option<&Inline>) -> Result<(), Error> {
        // A sentence's position is that of its first text (which skips blank text) or other content.
        if self.at.is_none() && !matches!(x, Inline::Text { .. } | Inline::Raw { .. }) {
            self.at = x.at().cloned();
        }
        match x {
            Inline::Text { text, at } => self.text(text, split, at, record),
            Inline::Code { text, at } => self.text(text, split, at, None),
            Inline::Raw { format, text, at } if format == TEXT || format == COMMONMARK => self.text(text, split, at, record),
            Inline::Raw { format, at, .. } => return Err(error(at, format!("this is markup for `{}`, which cannot be spoken", format))),
            Inline::Value { data, at } => self.text(&written_form(data, at)?, false, at, None),
            Inline::Hole { region, at } => return Err(error(at, format!("region {} is evaluated at runtime and has no value yet", region))),
            Inline::Emph(xs) | Inline::Strong(xs) | Inline::Link { inlines: xs, .. } => self.inlines(xs, split)?,
            Inline::Image { alt, at, .. } => self.text(alt, false, at, None),
            Inline::Math { tex, at, .. } => {
                return Err(error(at, format!("the formula `{}` has no spoken form: write it with `say`", tex)));
            }
            Inline::Break => self.say(" "),
            Inline::Say { shown, spoken } => {
                if self.at.is_none() {
                    self.at = x.at().cloned();
                }
                self.say(&format!(" {} ", spoken));
                let shown_text = plain(shown)?;
                self.shown.push_str(&shown_text);
            }
            Inline::Anchor { name, attrs, .. } => self.cut(AnchorKind::Point { name: name.clone(), attrs: attrs.clone() }),
            Inline::Span { name, attrs, inlines } => {
                self.cut(AnchorKind::Open { name: name.clone(), attrs: attrs.clone() });
                self.inlines(inlines, false)?;
                self.cut(AnchorKind::Close { name: name.clone() });
            }
            Inline::Pause { seconds, .. } => self.cut(AnchorKind::Pause { seconds: *seconds }),
            other => return Err(error(&other.at().cloned(), "this kind of content cannot be spoken yet")),
        }
        Ok(())
    }
}

/// Text cut at blank lines (a line holding only whitespace).
fn paragraphs(text: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    for (i, line) in lines.iter().enumerate() {
        let blank = line.trim().is_empty() && line.ends_with('\n') && i > 0 && lines[i - 1].ends_with('\n');
        if blank {
            if !out.last().unwrap().trim().is_empty() {
                out.push(String::new());
            }
        } else {
            out.last_mut().unwrap().push_str(line);
        }
    }
    out
}
