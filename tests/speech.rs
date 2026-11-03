//! Speech: sentences cut into pieces at their cues, and translations aligned with their originals. (Moved here
//! from lamlet-doc with the speech backend.)

use lamlet::embed::{Engine, SourceFile};
use lamlet_doc::{Doc, Error, COMMONMARK, TEXT};
use lamlet_present::speech::{self, AnchorKind};

const HEAD: &str = "$[Import* std]$[Import* content]";

fn lower(source: &str, prose: &str) -> Result<Doc, Error> {
    let mut e = Engine::new();
    e.set_resolver(lamlet_doc::resolver());
    let doc = e.check(SourceFile::new("s.lam", format!("{HEAD}{source}")))?;
    let mut instance = e.instance(&doc)?;
    lamlet_doc::evaluate_and_lower(&doc, &mut instance, prose)
}

#[test]
fn speech_cuts_pieces_at_anchors_and_spans() {
    let src = "\n# One {#one}\n\nFirst ${anchor \"m0\" {' {attr \"k\" \"v\"}}}words, ${span \"r\" {'} {text \"a range\"}} end${pause 0.5}.\n\n${say {math \"x^2\"} \"x squared\"} is shown.\n\n## Two {#two}\n\nLast.";
    let us = speech::utterances(&lower(src, COMMONMARK).unwrap()).unwrap();
    assert_eq!(us.len(), 3);
    assert_eq!(us[0].section, ["one"]);
    assert_eq!(us[0].pieces, ["First", "words,", "a range", "end", "."]);
    assert_eq!(us[0].spoken, "First words, a range end.");
    let kinds: Vec<(usize, String)> = us[0]
        .anchors
        .iter()
        .map(|a| {
            let k = match &a.kind {
                AnchorKind::Point { name, .. } => format!("point {name}"),
                AnchorKind::Open { name, .. } => format!("open {name}"),
                AnchorKind::Close { name } => format!("close {name}"),
                AnchorKind::Pause { seconds } => format!("pause {seconds}"),
            };
            (a.piece, k)
        })
        .collect();
    assert_eq!(kinds, [(1, "point m0".into()), (2, "open r".into()), (3, "close r".into()), (4, "pause 0.5".into())]);
    assert_eq!(us[1].spoken, "x squared is shown.");
    assert_eq!(us[1].shown, "x^2 is shown.");
    assert_eq!(us[2].section, ["one", "two"]);
    let e = speech::utterances(&lower("A ${math \"x\"}.", TEXT).unwrap()).unwrap_err();
    assert!(e.render().contains("has no spoken form"), "{}", e.render());
}

#[test]
fn unparsed_prose_speaks_one_utterance_per_paragraph() {
    let us = speech::utterances(&lower("\nOne line\ncontinues.\n\nTwo ${emph \"here\"}.\n", TEXT).unwrap()).unwrap();
    let spoken: Vec<&str> = us.iter().map(|u| u.spoken.as_str()).collect();
    assert_eq!(spoken, ["One line continues.", "Two here."]);
}

#[test]
fn translations_align_with_their_original() {
    let doc = |body: &str| speech::utterances(&lower(body, COMMONMARK).unwrap()).unwrap();
    let original = doc("\n# A {#a}\n\nOne ${anchor \"m\" {'}}cue.\n\nTwo.\n\n# B {#b}\n\nThree.");
    assert!(speech::align(&original, &doc("\n# 甲 {#a}\n\n一。\n\n二。\n\n# 乙 {#b}\n\n三。")).is_ok());
    let fewer = speech::align(&original, &doc("\n# 甲 {#a}\n\n一。\n\n# 乙 {#b}\n\n三。")).unwrap_err().render();
    assert!(fewer.contains("section `a` has 1 paragraphs here but 2 in the original"), "{fewer}");
    let cues = speech::align(&original, &doc("\n# 甲 {#a}\n\n一${anchor \"m\" {'}}。\n\n二。\n\n# 乙 {#b}\n\n三。")).unwrap_err().render();
    assert!(cues.contains("carries no anchors"), "{cues}");
    let renamed = speech::align(&original, &doc("\n# 甲 {#a}\n\n一。\n\n二。\n\n# 乙 {#c}\n\n三。")).unwrap_err().render();
    assert!(renamed.contains("section 2 is `c` here but `b`"), "{renamed}");
}

#[test]
fn the_command_writes_speech_and_checks_alignment() {
    let dir = std::env::temp_dir().join(format!("lamlet-present-speech-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (en, zh) = (dir.join("en.lam"), dir.join("zh.lam"));
    std::fs::write(&en, format!("{HEAD}\n# A {{#a}}\n\nOne ${{anchor \"m\" {{'}}}}cue.\n")).unwrap();
    std::fs::write(&zh, format!("{HEAD}\n# 甲 {{#a}}\n\n一。\n\n二。\n")).unwrap();
    let run = |args: &[&str]| std::process::Command::new(env!("CARGO_BIN_EXE_lamlet-present")).args(args).output().unwrap();
    let out = run(&["speech", en.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let j: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(j["utterances"][0]["anchors"][0]["kind"], "point");
    let out = run(&["speech", zh.to_str().unwrap(), "--align", en.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("2 paragraphs here but 1 in the original"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_deck_keeps_each_sentence_structure_for_hosts() {
    let src = format!(
        "{HEAD}$[Import* present]\n# Talk {{#talk fps=30}}\n\n## Room {{#c1-room kind=3d sources=\"a, b\"}}\n\n${{cue \"scene\" {{' {{attr \"demo\" \"room\"}}}}}}\n\nIt uses ${{say {{math \"R + TT\"}} \"R plus T T\"}} and ${{say {{code \"Main\"}} \"main\"}}${{during \"m0\" {{'}} {{text \" slowly\"}}}}.\n"
    );
    let p = lamlet_present::Presentation::compile(SourceFile::new("deck.lam", src), &[]).unwrap_or_else(|e| panic!("{}", e.render()));
    let d = p.deck.to_json().unwrap();
    assert_eq!(d["lamletDeck"], 1);
    assert_eq!(d["sections"][1]["attrs"], serde_json::json!([["kind", "3d"], ["sources", "a, b"]]));
    // A cue-only paragraph is a sentence with nothing spoken and its own line.
    let keys = &d["sentences"][0];
    assert_eq!((keys["spoken"].as_str(), keys["at"]["line"].as_u64()), (Some(""), Some(6)));
    assert_eq!(keys["inlines"][0]["type"], "anchor");
    let s = &d["sentences"][1];
    assert_eq!(s["spoken"], "It uses R plus T T and main slowly.");
    let types: Vec<&str> = s["inlines"].as_array().unwrap().iter().map(|i| i["type"].as_str().unwrap()).collect();
    assert_eq!(types, ["text", "say", "text", "say", "span", "text"]);
    assert_eq!(s["inlines"][1]["shown"][0]["tex"], "R + TT");
    assert_eq!(s["inlines"][3]["shown"][0]["text"], "Main");
}
