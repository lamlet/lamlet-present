//! Presentations end to end, without a real voice: the deck, the schedule from given measurements, curves,
//! narration files, cue vocabularies and key checks, slides, and the command.

use lamlet::embed::{Data, SourceFile};
use lamlet_present::*;

const HEAD: &str = "$[Import* std]$[Import* content]$[Import* present]\n";

fn compile(body: &str) -> Presentation {
    compile_with(body, &[]).unwrap_or_else(|e| panic!("{}", e.render()))
}

fn compile_with(body: &str, packages: &[(String, String)]) -> Result<Presentation, Error> {
    Presentation::compile(SourceFile::new("deck.lam", format!("{HEAD}{body}")), packages)
}

fn text_attrs(xs: &[(&str, &str)]) -> Vec<(String, Data)> {
    xs.iter().map(|(k, v)| (k.to_string(), Data::String(v.to_string()))).collect()
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
    assert_eq!(d.sections[1].attrs, text_attrs(&[("hold", "2")]));
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
    assert_eq!(s.cues[0].attrs, text_attrs(&[("k", "v")]));
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
fn cue_attributes_are_typed_data() {
    let body = "$[# rise [% {steady 1.0}]]\nIt ${cue \"up\" {' {attr \"curve\" \"rise\"} {attr \"seconds\" 1.5} {attr \"at\" {. x 1.0 y 2}}}}lights.";
    let mut p = compile(body);
    let s = p.schedule(&[n(2.0, &[0.0, 1.0])], &Options { gap: 0.0, fps: Some(2) }).unwrap();
    let c = &s.cues[0];
    let frames: Vec<u64> = c.samples.iter().map(|x| x.frame).collect();
    assert_eq!(frames, [2, 3, 4, 5], "a Float `seconds` times the animation");
    let j = s.to_json();
    assert_eq!(j["cues"][0]["attrs"], serde_json::json!({"curve": "rise", "seconds": 1.5, "at": {"x": 1.0, "y": 2}}));
    let deck = p.deck.to_json().unwrap();
    assert_eq!(deck["sentences"][0]["anchors"][0]["attrs"]["seconds"], 1.5);
    let speech = lamlet_present::speech::to_json(&[p.deck.sentences[0].utterance.clone()]);
    assert_eq!(speech["utterances"][0]["anchors"][0]["attrs"]["at"]["y"], 2);
    let curve = compile_with("It ${cue \"up\" {' {attr \"curve\" 1.0} {attr \"seconds\" 1.0}}}lights.", &[]).err().unwrap().render();
    assert!(curve.contains("`curve=1.0` is not a String naming a curve"), "{curve}");
    let mut p = compile("It ${cue \"up\" {' {attr \"curve\" \"rise\"} {attr \"seconds\" true}}}lights.\n$[# rise [% {steady 1.0}]]");
    let e = p.schedule(&[n(2.0, &[0.0, 1.0])], &Options { gap: 0.0, fps: Some(2) }).unwrap_err().render();
    assert!(e.contains("`seconds=true` is not a number of seconds"), "{e}");
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

fn float(x: f64) -> Data {
    Data::Float(x)
}

fn strs(xs: &[&str]) -> Data {
    Data::List(xs.iter().map(|x| Data::String(x.to_string())).collect())
}

#[test]
fn a_vocabulary_types_cue_attributes() {
    let body = "$[Import* lights]\nNow ${cue \"c\" {' {intensity 0.5} {intensityRange 0.0 1.0} {cameraYaw 24.0} {steps 3} {visible false} {tint {' 1.0 0.5}} modeLightOnly {label \"hi\"}}}look.";
    let mut p = compile_with(body, &lights()).unwrap_or_else(|e| panic!("{}", e.render()));
    let s = p.build(&mut EstimatedVoice::default(), &Options::default()).unwrap().1;
    let range = Data::Record([("from".to_string(), float(0.0)), ("to".to_string(), float(1.0))].into());
    let expected = vec![
        ("intensity".to_string(), float(0.5)),
        ("intensity".to_string(), range),
        ("camera.yaw".to_string(), float(24.0)),
        ("steps".to_string(), Data::Int(3)),
        ("visible".to_string(), Data::Bool(false)),
        ("tint".to_string(), Data::List(vec![float(1.0), float(0.5)])),
        ("mode".to_string(), Data::String("light-only".into())),
        ("ui.label".to_string(), Data::String("hi".into())),
    ];
    assert_eq!(s.cues[0].attrs, expected);
    assert_eq!(s.to_json()["cues"][0]["attrs"]["steps"], 3);
    let typo = compile_with("$[Import* lights]\nNow ${cue \"c\" {' {intensty 0.5}}}look.", &lights()).err().unwrap().render();
    assert!(typo.contains("deck.lam:3:19: name error: unknown name `intensty`"), "{typo}");
    let wrong = compile_with("$[Import* lights]\nNow ${cue \"c\" {' {intensity \"bright\"}}}look.", &lights()).err().unwrap().render();
    assert!(wrong.contains("type error"), "{wrong}");
    let clash = vocab::keys_from_json(r#"[{"name": "camera.yaw", "kind": "bool"}, {"name": "cameraYaw", "kind": "int"}]"#).unwrap();
    assert!(vocab::package("x", &clash).unwrap_err().contains("two functions would be named `cameraYaw`"));
    assert!(vocab::keys_from_json(r#"[{"name": "x", "kind": "colour"}]"#).unwrap_err().contains("unknown kind"));
}

const RICH: &str = r#"[
    {"name": "panel", "kind": "text", "options": ["frame", "light-only"], "closed": true},
    {"name": "passes", "kind": "names", "options": ["gbuffer", "shade"], "closed": true},
    {"name": "at", "kind": "type", "type": "{Struct {' {Field \"x\" Float} {Field \"y\" Float}}}"},
    {"name": "mark", "kind": "type", "type": "{Variant {' {Field \"none\" Unit} {Field \"box\" {List Float}}}}"},
    {"name": "param", "kind": "float", "prefix": true},
    {"name": "say", "kind": "any"},
    {"name": "target", "kind": "int", "on": "heading", "levels": [2], "default": 60},
    {"name": "sources", "kind": "names", "on": "heading", "levels": [2]},
    {"name": "kind", "kind": "text", "options": ["2d", "3d"], "closed": true, "on": "heading", "levels": [3], "default": "2d"},
    {"name": "focus", "kind": "type", "type": "{List Float}", "on": "heading", "levels": [3]},
    {"name": "fps", "kind": "int", "on": "both"}
]"#;

fn rich() -> (Vec<(String, String)>, Schema) {
    let keys = vocab::keys_from_json(RICH).unwrap();
    let packages = vec![("rich".to_string(), vocab::package("Rich", &keys).unwrap())];
    let schema = Schema::new(&keys, &packages).unwrap_or_else(|e| panic!("{e}"));
    (packages, schema)
}

#[test]
fn closed_options_type_keys_and_families() {
    let (packages, schema) = rich();
    let source = vocab::package("Rich", &vocab::keys_from_json(RICH).unwrap()).unwrap();
    // A closed text key has only its options; a heading key has no functions.
    assert!(source.contains("$[# panelFrame [% {attr \"panel\" \"frame\"}]]"), "{source}");
    assert!(!source.contains("{panel s}") && !source.contains("target") && !source.contains("{say"), "{source}");
    let body = "$[Import* rich]\nNow ${cue \"c\" {' panelLightOnly {passes {' \"shade\"}} {at {. x 1.0 y 2.0}} {mark {#box {' 0.5}}} {param \"exposure\" 2.0} {paramRange \"gain\" 0.0 1.0} {fps 30}}}look.";
    let mut p = compile_with(body, &packages).unwrap_or_else(|e| panic!("{}", e.render()));
    p.apply_keys(&schema).unwrap_or_else(|es| panic!("{}", es[0].render()));
    let attrs = &p.deck.sentences[0].utterance.anchors[0];
    let speech::AnchorKind::Point { attrs, .. } = &attrs.kind else { panic!() };
    let keys: Vec<&str> = attrs.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["panel", "passes", "at", "mark", "param.exposure", "param.gain", "fps"]);
    assert_eq!(attrs[3].1, Data::Variant("box".into(), Box::new(Data::List(vec![float(0.5)]))));
    // A wrong option is an unknown name, and a wrong value of a type key a type error.
    let e = compile_with("$[Import* rich]\nNow ${cue \"c\" {' panelAlbedo}}look.", &packages).err().unwrap().render();
    assert!(e.contains("unknown name `panelAlbedo`"), "{e}");
    let e = compile_with("$[Import* rich]\nNow ${cue \"c\" {' {at {. x 1.0}}}}look.", &packages).err().unwrap().render();
    assert!(e.contains("type error"), "{e}");
    assert!(vocab::keys_from_json(r#"[{"name": "x", "kind": "float", "closed": true}]"#).unwrap_err().contains("closed"));
    assert!(vocab::keys_from_json(r#"[{"name": "x", "kind": "type"}]"#).unwrap_err().contains("needs a \"type\""));
    assert!(vocab::keys_from_json(r#"[{"name": "x", "kind": "int", "default": 1}]"#).unwrap_err().contains("heading key"));
    let bad = vocab::keys_from_json(r#"[{"name": "x", "kind": "type", "type": "3"}]"#).unwrap();
    assert!(Schema::new(&bad, &[]).unwrap_err().contains("a key's type"));
    let bad = vocab::keys_from_json(r#"[{"name": "x", "kind": "int", "on": "heading", "default": "many"}]"#).unwrap();
    assert!(Schema::new(&bad, &[]).unwrap_err().contains("the default of `x`"));
}

#[test]
fn keys_refuse_raw_attributes_and_report_every_problem() {
    let (packages, schema) = rich();
    let body = "$[Import* rich]\nOne ${cue \"a\" {' {attr \"panel\" \"albedo\"} {attr \"pannel\" \"frame\"}}}two.\n\n\
        Three ${cue \"b\" {' {attr \"at\" {. x 1}} {attr \"passes\" {' \"shade\" \"blur\"}} {attr \"param\" 1.0} {attr \"say\" 3} {attr \"seconds\" 1.0}}}four.";
    let mut p = compile_with(body, &packages).unwrap_or_else(|e| panic!("{}", e.render()));
    let es: Vec<String> = p.apply_keys(&schema).unwrap_err().iter().map(|e| e.render()).collect();
    let all = es.join("\n");
    assert_eq!(es.len(), 5, "{all}");
    assert!(all.contains("deck.lam:3:1") && all.contains("the cue `a`: `panel`: `albedo` is not one of frame, light-only"), "{all}");
    assert!(all.contains("`pannel` is not a cue attribute this host knows"), "{all}");
    assert!(all.contains("deck.lam:5:1") && all.contains("`at={\"x\":1}` is not a value of type `{x: Float, y: Float}`"), "{all}");
    assert!(all.contains("`passes`: `blur` is not one of gbuffer, shade"), "{all}");
    // A family needs a member's name.
    assert!(all.contains("`param` is not a cue attribute"), "{all}");
    // Without cue keys, cues are not checked.
    let headings = Schema::new(&vocab::keys_from_json(r#"[{"name": "t", "kind": "int", "on": "heading"}]"#).unwrap(), &[]).unwrap();
    let mut p = compile("One ${cue \"a\" {' {attr \"anything\" 1}}}two.");
    p.apply_keys(&headings).unwrap();
}

#[test]
fn heading_attributes_are_read_by_their_keys() {
    let (packages, schema) = rich();
    let body = "# Talk\n\n## One {#c1 sources=\"frame-order, settings\" fps=30 hold=2}\n\nA.\n\n### Room {#r focus=[0.5,1]}\n\nB.\n\n## Two {#c2 target=220}\n\nC.\n";
    let mut p = compile_with(body, &packages).unwrap_or_else(|e| panic!("{}", e.render()));
    p.apply_keys(&schema).unwrap_or_else(|es| panic!("{}", es[0].render()));
    let s = &p.deck.sections;
    assert_eq!(s[0].attrs, vec![]);
    let c1: Vec<(String, Data)> = vec![
        ("sources".into(), strs(&["frame-order", "settings"])),
        ("fps".into(), Data::Int(30)),
        ("hold".into(), Data::String("2".into())),
        ("target".into(), Data::Int(60)),
    ];
    assert_eq!(s[1].attrs, c1);
    assert_eq!(s[2].attrs, vec![("focus".into(), Data::List(vec![float(0.5), float(1.0)])), ("kind".into(), Data::String("2d".into()))]);
    assert_eq!(s[3].attrs, vec![("target".into(), Data::Int(220))]);
    // The hold still times the section, and the schedule writes typed values.
    let sched = p.build(&mut EstimatedVoice::default(), &Options::default()).unwrap().1;
    assert_eq!(sched.to_json()["sections"][1]["attrs"]["target"], 60);
    let body = "# Talk\n\n## One {#c1 target=many kind=3d}\n\nA.\n\n### Room {#r kind=4d focus=x}\n\nB.\n";
    let mut p = compile_with(body, &packages).unwrap();
    let all: Vec<String> = p.apply_keys(&schema).unwrap_err().iter().map(|e| e.render()).collect();
    let all = all.join("\n");
    assert!(all.contains("section `c1`: `target=many` is not an integer"), "{all}");
    assert!(all.contains("section `c1`: `kind` is not an attribute of a level-2 heading"), "{all}");
    assert!(all.contains("section `r`: `kind`: `4d` is not one of 2d, 3d"), "{all}");
    assert!(all.contains("section `r`: `focus=x` is not a value of type `List(Float)`"), "{all}");
}

#[test]
fn recursive_types_check_data() {
    let keys = vocab::keys_from_json(r#"[{"name": "tree", "kind": "type", "type": "Tree", "imports": ["trees"]}]"#).unwrap();
    let trees = "$[Import* std]$[Type Tree roll unroll {Variant {' {Field \"leaf\" Int} {Field \"node\" {List Tree}}}}]\n".to_string();
    let packages = vec![("trees".to_string(), trees)];
    let schema = Schema::new(&keys, &packages).unwrap_or_else(|e| panic!("{e}"));
    let leaf = |n| Data::Variant("leaf".into(), Box::new(Data::Int(n)));
    let good = vec![("tree".to_string(), Data::Variant("node".into(), Box::new(Data::List(vec![leaf(1), leaf(2)]))))];
    assert!(schema.check_cue(&good).is_empty());
    let bad = vec![("tree".to_string(), Data::Variant("node".into(), Box::new(Data::List(vec![Data::Int(1)]))))];
    assert_eq!(schema.check_cue(&bad).len(), 1);
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
    // With the host's keys, every unknown attribute is reported.
    let keys = dir.join("keys.json");
    std::fs::write(&keys, r#"[{"name": "j", "kind": "float"}, {"name": "hold", "kind": "float", "on": "heading"}]"#).unwrap();
    let out = run(&["deck", deck.to_str().unwrap(), "--keys", keys.to_str().unwrap()]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(err.contains("the cue `x`: `k` is not a cue attribute this host knows"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}
