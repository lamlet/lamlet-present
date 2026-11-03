//! Presentations end to end, without a real voice: the deck, the schedule from given measurements, curves,
//! narration files, cue vocabularies, slides, and the command.

use lamlet::embed::SourceFile;
use lamlet_present::*;

const HEAD: &str = "$[Import* std]$[Import* content]$[Import* present]\n";

fn compile(body: &str) -> Presentation {
    compile_with(body, &[]).unwrap_or_else(|e| panic!("{}", e.render()))
}

fn compile_with(body: &str, packages: &[(String, String)]) -> Result<Presentation, Error> {
    Presentation::compile(SourceFile::new("deck.lam", format!("{HEAD}{body}")), packages)
}

fn n(seconds: f64, pieces: &[f64]) -> Narration {
    Narration { seconds, pieces: pieces.to_vec() }
}

const DECK: &str = "# Talk\n\n## First {#a hold=2}\n\nOne ${cue \"x\" {' {attr \"k\" \"v\"}}}two.\n\nThree${pause 1.0} four.\n\n## Second\n\nFive.\n";

#[test]
fn headings_are_sections_and_paragraphs_are_sentences() {
    let p = compile(DECK);
    let d = &p.deck;
    assert_eq!(d.title, "Talk");
    let ids: Vec<(&str, u8)> = d.sections.iter().map(|s| (s.id.as_str(), s.level)).collect();
    assert_eq!(ids, [("talk", 1), ("a", 2), ("second", 2)]);
    assert_eq!(d.sections[1].attrs, [("hold".to_string(), "2".to_string())]);
    assert_eq!(d.sections[2].path, ["talk", "second"]);
    let spoken: Vec<&str> = d.sentences.iter().map(|s| s.utterance.spoken.as_str()).collect();
    assert_eq!(spoken, ["One two.", "Three four.", "Five."]);
    assert_eq!(d.sections[1].sentences, 0..2);
    assert_eq!(d.sentences[0].utterance.pieces, ["One", "two."]);
}

#[test]
fn the_schedule_follows_the_measured_speech() {
    let mut p = compile(DECK);
    let narration = [n(2.0, &[0.0, 1.0]), n(2.0, &[0.0, 1.0]), n(1.0, &[0.0])];
    let s = p.schedule(&narration, &Options { gap: 0.5, fps: Some(10) }).unwrap();
    let times: Vec<(f64, f64)> = s.sentences.iter().map(|x| (x.start, x.end)).collect();
    // The pause adds a second inside the second sentence; the first section holds 2 seconds after its gap.
    assert_eq!(times, [(0.0, 2.0), (2.5, 5.5), (8.0, 9.0)]);
    assert_eq!(s.sentences[1].pieces, [2.5, 4.5]);
    let sections: Vec<(&str, f64, f64)> = s.sections.iter().map(|x| (x.id.as_str(), x.start, x.end)).collect();
    assert_eq!(sections, [("talk", 0.0, 0.0), ("a", 0.0, 8.0), ("second", 8.0, 9.0)]);
    assert_eq!(s.seconds, 9.0);
    assert_eq!(s.cues.len(), 1);
    assert_eq!((s.cues[0].name.as_str(), s.cues[0].start, s.cues[0].end), ("x", 1.0, None));
    assert_eq!(s.cues[0].attrs, [("k".to_string(), "v".to_string())]);
    let j = s.to_json();
    assert_eq!(j["lamletSchedule"], 1);
    assert_eq!(j["cues"][0]["startFrame"], 10);
    assert_eq!(j["sentences"][2]["endFrame"], 90);
}

#[test]
fn a_cue_over_words_lasts_from_its_first_word_to_its_last() {
    let mut p = compile("Some ${duringText \"glow\" {'} \"warm light\"} here.");
    assert_eq!(p.deck.sentences[0].utterance.pieces, ["Some", "warm light", "here."]);
    let s = p.schedule(&[n(3.0, &[0.0, 1.0, 2.0])], &Options::default()).unwrap();
    assert_eq!((s.cues[0].start, s.cues[0].end), (1.0, Some(2.0)));
}

#[test]
fn a_narration_that_does_not_fit_is_an_error_at_its_sentence() {
    let mut p = compile(DECK);
    let e = p.schedule(&[n(2.0, &[0.0]), n(2.0, &[0.0, 1.0]), n(1.0, &[0.0])], &Options::default()).unwrap_err().render();
    assert!(e.contains("deck.lam:6:1:") && e.contains("1 piece times for 2 pieces"), "{e}");
    let e = p.schedule(&[n(1.0, &[0.0, 2.0]), n(2.0, &[0.0, 1.0]), n(1.0, &[0.0])], &Options::default()).unwrap_err().render();
    assert!(e.contains("must rise from 0 to the duration"), "{e}");
}

const ANIMATED: &str = "$[# rise [% {tween 0.0 6.0 1.0 smooth}]]\nIt ${animateText \"up\" \"rise\" {'} \"lights up\"} now.";

#[test]
fn animations_are_sampled_at_every_frame_they_cover() {
    let mut p = compile(ANIMATED);
    let s = p.schedule(&[n(3.0, &[0.0, 0.5, 2.0])], &Options { gap: 0.4, fps: Some(10) }).unwrap();
    let c = &s.cues[0];
    assert_eq!((c.start, c.end), (0.5, Some(2.0)));
    let frames: Vec<u64> = c.samples.iter().map(|x| x.frame).collect();
    assert_eq!(frames, (5..=20).collect::<Vec<_>>());
    assert_eq!(c.samples[0].value, 0.0);
    assert!(c.samples.windows(2).all(|w| w[0].value <= w[1].value), "a smooth tween only rises");
    assert_eq!(c.samples.last().unwrap().value, 6.0, "and holds its end after its duration");
    // The same document and measurements give the same samples.
    let again = p.schedule(&[n(3.0, &[0.0, 0.5, 2.0])], &Options { gap: 0.4, fps: Some(10) }).unwrap();
    assert_eq!(s, again);
    let e = p.schedule(&[n(3.0, &[0.0, 0.5, 2.0])], &Options::default()).unwrap_err().render();
    assert!(e.contains("needs a frame rate"), "{e}");
}

#[test]
fn curves_are_checked_before_any_speech() {
    let missing = compile_with("It ${animateText \"up\" \"rize\" {'} \"lights\"}.", &[]).err().unwrap().render();
    assert!(missing.contains("deck.lam:2:1:") && missing.contains("no static top-level binding `rize`"), "{missing}");
    let wrong = compile_with("$[# label \"x\"]\nIt ${animateText \"up\" \"label\" {'} \"lights\"}.", &[]).err().unwrap().render();
    assert!(wrong.contains("is not `Fn([Float], Float)`"), "{wrong}");
    let point = compile_with("$[# rise [% {steady 1.0}]]\nIt ${cue \"up\" {' {attr \"curve\" \"rise\"}}}lights.", &[]).err().unwrap().render();
    assert!(point.contains("needs `seconds`"), "{point}");
}

#[test]
fn rebuilding_never_synthesizes_again() {
    let mut p = compile(DECK);
    let mut voice = Cached::new(EstimatedVoice::default());
    let (_, first) = p.build(&mut voice, &Options::default()).unwrap();
    let (_, second) = p.build(&mut voice, &Options::default()).unwrap();
    assert_eq!(first, second);
    assert_eq!(voice.syntheses, 3);
}

#[test]
fn narration_files_round_trip_and_refuse_stale_measurements() {
    let mut p = compile(DECK);
    let (narration, _) = p.build(&mut EstimatedVoice::default(), &Options::default()).unwrap();
    let json = narration_json(&p.deck, &narration);
    assert_eq!(narration_from_json(&p.deck, &json).unwrap(), narration);
    let mut stale = json.clone();
    stale["sentences"][1]["spoken"] = "Three five.".into();
    let e = narration_from_json(&p.deck, &stale).unwrap_err().render();
    assert!(e.contains("was measured for \"Three five.\"") && e.contains("synthesize it again"), "{e}");
    let mut short = json;
    short["sentences"].as_array_mut().unwrap().pop();
    assert!(narration_from_json(&p.deck, &short).unwrap_err().render().contains("2 sentences but the presentation 3"));
}

#[test]
fn the_script_lists_what_to_synthesize() {
    let p = compile(DECK);
    let j = p.script_json();
    assert_eq!(j["lamletScript"], 1);
    assert_eq!(j["sentences"][0]["pieces"], serde_json::json!(["One", "two."]));
    assert_eq!(j["sentences"][2]["section"], "second");
}

const KEYS: &str = r#"[
    {"name": "intensity", "kind": "float"},
    {"name": "camera.yaw", "kind": "float"},
    {"name": "steps", "kind": "int"},
    {"name": "visible", "kind": "bool"},
    {"name": "tint", "kind": "vector"},
    {"name": "mode", "kind": "text", "options": ["albedo", "light-only"]},
    {"name": "label", "kind": "text", "key": "ui.label"}
]"#;

fn lights() -> Vec<(String, String)> {
    let keys = vocab::keys_from_json(KEYS).unwrap();
    vec![("lights".to_string(), vocab::package("Lights", &keys).unwrap())]
}

#[test]
fn a_vocabulary_types_cue_attributes() {
    let body = "$[Import* lights]\nNow ${cue \"c\" {' {intensity 0.5} {intensityRange 0.0 1.0} {cameraYaw 24.0} {steps 3} {visible false} {tint {' 1.0 0.5}} modeLightOnly {label \"hi\"}}}look.";
    let mut p = compile_with(body, &lights()).unwrap_or_else(|e| panic!("{}", e.render()));
    let s = p.build(&mut EstimatedVoice::default(), &Options::default()).unwrap().1;
    let attrs: Vec<(String, String)> = s.cues[0].attrs.clone();
    let expected = [
        ("intensity", "0.5"),
        ("intensity", "0.0..1.0"),
        ("camera.yaw", "24.0"),
        ("steps", "3"),
        ("visible", "false"),
        ("tint", "1.0,0.5"),
        ("mode", "light-only"),
        ("ui.label", "hi"),
    ];
    assert_eq!(attrs, expected.map(|(k, v)| (k.to_string(), v.to_string())));
    let typo = compile_with("$[Import* lights]\nNow ${cue \"c\" {' {intensty 0.5}}}look.", &lights()).err().unwrap().render();
    assert!(typo.contains("deck.lam:3:19: name error: unknown name `intensty`"), "{typo}");
    let wrong = compile_with("$[Import* lights]\nNow ${cue \"c\" {' {intensity \"bright\"}}}look.", &lights()).err().unwrap().render();
    assert!(wrong.contains("type error"), "{wrong}");
    let clash = vocab::keys_from_json(r#"[{"name": "camera.yaw", "kind": "bool"}, {"name": "cameraYaw", "kind": "int"}]"#).unwrap();
    assert!(vocab::package("x", &clash).unwrap_err().contains("two functions would be named `cameraYaw`"));
    assert!(vocab::keys_from_json(r#"[{"name": "x", "kind": "colour"}]"#).unwrap_err().contains("unknown kind"));
}

#[test]
fn slides_show_each_section_with_its_notes() {
    let p = compile(DECK);
    let html = slides::write(&p.deck).unwrap();
    assert!(html.starts_with("<!doctype html>"), "{html}");
    assert!(html.contains("<title>Talk</title>"));
    assert!(html.contains("<section class=\"slide\" id=\"a\">\n<h2>First</h2>\n<p>One two.</p>"), "{html}");
    assert!(html.contains("<aside class=\"notes\">\n<p>One two.</p>\n<p>Three four.</p>\n</aside>"), "{html}");
}

#[test]
fn the_example_builds() {
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/lighting.lam")).unwrap();
    let mut p = Presentation::compile(SourceFile::new("lighting.lam", source), &[]).unwrap_or_else(|e| panic!("{}", e.render()));
    let (_, s) = p.build(&mut EstimatedVoice::default(), &Options { gap: 0.4, fps: Some(30) }).unwrap();
    let names: Vec<&str> = s.cues.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["scene", "light-up", "formula", "orbit", "radius"]);
    assert!(s.cues.iter().filter(|c| !c.samples.is_empty()).count() == 2);
    assert_eq!(s.sections.last().unwrap().title, "A bright room");
}

#[test]
fn the_command_schedules_from_a_narration_file() {
    let dir = std::env::temp_dir().join(format!("lamlet-present-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let deck = dir.join("deck.lam");
    std::fs::write(&deck, format!("{HEAD}{DECK}")).unwrap();
    let run = |args: &[&str]| std::process::Command::new(env!("CARGO_BIN_EXE_lamlet-present")).args(args).output().unwrap();
    let narration = dir.join("narration.json");
    // The narration needs no frame rate, even for a deck with animations.
    let animated = dir.join("animated.lam");
    std::fs::write(&animated, format!("{HEAD}{ANIMATED}")).unwrap();
    assert!(run(&["narration", animated.to_str().unwrap()]).status.success());
    assert!(run(&["narration", deck.to_str().unwrap(), "-o", narration.to_str().unwrap()]).status.success());
    let out = run(&["schedule", deck.to_str().unwrap(), "--narration", narration.to_str().unwrap(), "--fps", "30"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let j: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(j["fps"], 30);
    assert_eq!(j["sections"].as_array().unwrap().len(), 3);
    let out = run(&["source"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout), PRESENT_SOURCE);
    let out = run(&["schedule", deck.to_str().unwrap(), "--fps", "x"]);
    assert_eq!(out.status.code(), Some(1));
    std::fs::remove_dir_all(&dir).ok();
}
