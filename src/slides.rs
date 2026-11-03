//! A deck as an HTML slide show: one slide per section, with its narration as speaker notes.
//!
//! The page needs nothing else: arrow keys, Page Up and Down, and Space move between slides, `n` shows the
//! notes, and the address's `#id` names the slide. A slide shows its section's blocks as `lamlet-doc`'s HTML
//! backend writes them; cues and pauses show nothing.

use crate::deck::Deck;
use lamlet_doc::backend::html;
use lamlet_doc::ir::Doc;
use lamlet_doc::Error;

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

const STYLE: &str = "\
:root { color-scheme: light dark; --bg: #fdfcf8; --fg: #1d1d1f; --muted: #6b6b70; }
@media (prefers-color-scheme: dark) { :root { --bg: #16161a; --fg: #ececf0; --muted: #9a9aa2; } }
html, body { margin: 0; height: 100%; background: var(--bg); color: var(--fg); font: 22px/1.45 system-ui, sans-serif; }
section.slide { display: none; box-sizing: border-box; min-height: 100vh; padding: 6vh 8vw; }
section.slide.current { display: block; }
section.slide h1 { font-size: 2.2em; margin: 0 0 0.6em; } section.slide h2 { font-size: 1.6em; margin: 0 0 0.6em; }
img { max-width: 100%; } table { border-collapse: collapse; } td, th { border: 1px solid var(--muted); padding: 0.2em 0.6em; }
pre { overflow-x: auto; } aside.notes { display: none; margin-top: 2em; color: var(--muted); font-size: 0.8em; }
body.notes aside.notes { display: block; }
nav.progress { position: fixed; right: 1em; bottom: 0.6em; color: var(--muted); font-size: 0.7em; }
";

const SCRIPT: &str = "\
const slides = [...document.querySelectorAll('section.slide')];
let i = Math.max(0, slides.findIndex(s => '#' + s.id === location.hash));
function show(k) {
  i = Math.min(slides.length - 1, Math.max(0, k));
  slides.forEach((s, j) => s.classList.toggle('current', j === i));
  if (slides[i].id) history.replaceState(null, '', '#' + slides[i].id);
  document.querySelector('nav.progress').textContent = (i + 1) + ' / ' + slides.length;
}
addEventListener('keydown', e => {
  if (['ArrowRight', 'ArrowDown', 'PageDown', ' '].includes(e.key)) { show(i + 1); e.preventDefault(); }
  if (['ArrowLeft', 'ArrowUp', 'PageUp'].includes(e.key)) { show(i - 1); e.preventDefault(); }
  if (e.key === 'n') document.body.classList.toggle('notes');
});
show(i);
";

/// The deck as an HTML page.
pub fn write(deck: &Deck) -> Result<String, Error> {
    let mut slides = String::new();
    for s in &deck.sections {
        let id = if s.id.is_empty() { String::new() } else { format!(" id=\"{}\"", escape(&s.id)) };
        slides.push_str(&format!("<section class=\"slide\"{}>\n", id));
        if s.level > 0 {
            let h = if s.level == 1 { "h1" } else { "h2" };
            slides.push_str(&format!("<{h}>{}</{h}>\n", escape(&s.title)));
        }
        slides.push_str(&html::write(&Doc { blocks: s.blocks.clone() })?);
        let notes: Vec<String> = s.sentences.clone().map(|k| format!("<p>{}</p>", escape(&deck.sentences[k].utterance.spoken))).collect();
        if !notes.is_empty() {
            slides.push_str(&format!("\n<aside class=\"notes\">\n{}\n</aside>", notes.join("\n")));
        }
        slides.push_str("\n</section>\n");
    }
    Ok(format!(
        "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{}</title>\n<style>\n{}</style>\n</head>\n<body>\n{}<nav class=\"progress\"></nav>\n<script>\n{}</script>\n</body>\n</html>\n",
        escape(if deck.title.is_empty() { "Slides" } else { &deck.title }),
        STYLE,
        slides,
        SCRIPT
    ))
}
