//! The `lamlet-present` command, a Lamlet extension: `lamlet present ...` runs it.

use lamlet::embed::SourceFile;
use lamlet_present::{narration_from_json, narration_json, slides, vocab, EstimatedVoice, Options, Presentation, Schema};
use std::process::ExitCode;

const USAGE: &str = "\
usage (as a Lamlet extension, `lamlet present ...` is `lamlet-present ...`):
  lamlet present script    FILE [--package NAME=PATH]... [-o OUT]
      What to synthesize: every sentence, cut into pieces at its cues (JSON).
  lamlet present schedule  FILE [--narration NARRATION.json] [--fps N] [--gap SECONDS] [--package NAME=PATH]... [-o OUT]
      Every section, sentence, and cue with its time (JSON). Times come from the narration a voice measured;
      without --narration, from an estimate (no audio). With --fps, times also have frames and animations are
      sampled at every frame.
  lamlet present narration FILE [--package NAME=PATH]... [-o OUT]
      The estimated narration, in the form --narration reads: a template for a host's voice to fill in.
  lamlet present deck     FILE [--package NAME=PATH]... [-o OUT]
      The deck with no timing: every section (id, level, title, attributes) and sentence (spoken text, pieces,
      cues, and its content as Lamlet's IR), for a host that times it by its own rules (JSON).
  lamlet present speech   FILE [--align ORIGINAL] [--package NAME=PATH]... [-o OUT]
      Every sentence as spoken: its spoken and shown text, pieces, and cues (JSON). With --align, FILE is a
      translation of ORIGINAL: it must have the same sections and paragraph counts, and no cues.
  lamlet present slides    FILE [--package NAME=PATH]... [-o OUT]
      An HTML slide show: one slide per section, the narration as speaker notes (press n).
  lamlet present vocab     NAME KEYS.json [--package NAME=PATH]... [-o OUT]
      A package of typed cue attributes from a host's keys:
      [{\"name\": ..., \"kind\": \"float\"|\"int\"|\"bool\"|\"vector\"|\"text\"|\"names\"|\"type\"|\"any\",
        \"options\": [...], \"closed\": true, \"type\": \"LAMLET TYPE\", \"imports\": [...], \"key\": ...,
        \"prefix\": true, \"on\": \"cue\"|\"heading\"|\"both\", \"levels\": [...], \"default\": ...}].
  lamlet present source    [-o OUT]
      The `present` package's Lamlet source, for other tools: lamlet build deck.lam --package present=present.lam

A presentation is a Lamlet document whose prose is CommonMark: headings are sections, paragraphs are
sentences. The packages std, content, and present are always available. Diagnostics go to stderr.

--keys KEYS.json (script, schedule, narration, deck, speech, slides): check every cue attribute against the
host's keys, and read heading attributes by them (typed, with defaults). Every problem is reported.";

struct Args {
    positional: Vec<String>,
    packages: Vec<(String, String)>,
    out: Option<String>,
    narration: Option<String>,
    align: Option<String>,
    fps: Option<u32>,
    gap: Option<f64>,
    keys: Option<String>,
}

fn parse(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut a = Args { positional: vec![], packages: vec![], out: None, narration: None, align: None, fps: None, gap: None, keys: None };
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        let mut value = |what: &str| args.next().ok_or(format!("{arg} expects {what}"));
        match arg.as_str() {
            "--package" => {
                let v = value("NAME=PATH")?;
                let (name, path) = v.split_once('=').ok_or("--package expects NAME=PATH")?;
                let source = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
                a.packages.push((name.to_string(), source));
            }
            "-o" | "--out" => a.out = Some(value("a file")?),
            "--narration" => a.narration = Some(value("a file")?),
            "--align" => a.align = Some(value("a file")?),
            "--keys" => a.keys = Some(value("a file")?),
            "--fps" => a.fps = Some(value("a number")?.parse().map_err(|_| "--fps expects a whole number")?),
            "--gap" => a.gap = Some(value("seconds")?.parse().map_err(|_| "--gap expects seconds")?),
            "-h" | "--help" => return Err(String::new()),
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            _ => a.positional.push(arg),
        }
    }
    Ok(a)
}

fn write(out: &Option<String>, text: &str) -> Result<(), String> {
    match out {
        Some(path) => std::fs::write(path, text).map_err(|e| format!("{path}: {e}")),
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

fn pretty(j: &serde_json::Value) -> String {
    serde_json::to_string_pretty(j).unwrap() + "\n"
}

/// A presentation, checked against the host's keys when there are any.
fn compile_keyed(file: &str, packages: &[(String, String)], schema: Option<&Schema>) -> Result<Presentation, String> {
    let source = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
    let mut p = Presentation::compile(SourceFile::new(file, source), packages).map_err(|e| e.render())?;
    if let Some(schema) = schema {
        p.apply_keys(schema).map_err(|es| es.iter().map(|e| e.render().trim_end().to_string()).collect::<Vec<_>>().join("\n"))?;
    }
    Ok(p)
}

fn schema(keys: &Option<String>, packages: &[(String, String)]) -> Result<Option<Schema>, String> {
    let Some(path) = keys else { return Ok(None) };
    let json = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let keys = vocab::keys_from_json(&json).map_err(|e| format!("{path}: {e}"))?;
    Schema::new(&keys, packages).map(Some).map_err(|e| format!("{path}: {e}"))
}

fn run() -> Result<(), String> {
    let mut argv = std::env::args().skip(1);
    let command = argv.next().ok_or_else(String::new)?;
    let a = parse(argv)?;
    let options = Options { fps: a.fps, gap: a.gap.unwrap_or(Options::default().gap) };
    let schema = schema(&a.keys, &a.packages)?;
    let compile = |file: &str, packages: &[(String, String)]| compile_keyed(file, packages, schema.as_ref());
    match (command.as_str(), a.positional.as_slice()) {
        ("script", [file]) => write(&a.out, &pretty(&compile(file, &a.packages)?.script_json())),
        ("narration", [file]) => {
            let p = compile(file, &a.packages)?;
            let narration = p.synthesize(&mut EstimatedVoice::default()).map_err(|e| e.render())?;
            write(&a.out, &pretty(&narration_json(&p.deck, &narration)))
        }
        ("schedule", [file]) => {
            let mut p = compile(file, &a.packages)?;
            let schedule = match &a.narration {
                Some(path) => {
                    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
                    let json = serde_json::from_str(&text).map_err(|e| format!("{path}: not JSON: {e}"))?;
                    let narration = narration_from_json(&p.deck, &json).map_err(|e| format!("{path}: {}", e.render()))?;
                    p.schedule(&narration, &options)
                }
                None => p.build(&mut EstimatedVoice::default(), &options).map(|(_, s)| s),
            }
            .map_err(|e| e.render())?;
            write(&a.out, &pretty(&schedule.to_json()))
        }
        ("deck", [file]) => write(&a.out, &pretty(&compile(file, &a.packages)?.deck.to_json().map_err(|e| e.render())?)),
        ("speech", [file]) => {
            let utterances = |p: &Presentation| p.deck.sentences.iter().map(|s| s.utterance.clone()).collect::<Vec<_>>();
            let this = utterances(&compile(file, &a.packages)?);
            if let Some(original) = &a.align {
                let theirs = utterances(&compile(original, &a.packages)?);
                lamlet_present::speech::align(&theirs, &this).map_err(|e| e.render())?;
            }
            write(&a.out, &pretty(&lamlet_present::speech::to_json(&this)))
        }
        ("slides", [file]) => write(&a.out, &slides::write(&compile(file, &a.packages)?.deck).map_err(|e| e.render())?),
        ("source", []) => write(&a.out, lamlet_present::PRESENT_SOURCE),
        ("vocab", [name, keys]) => {
            let json = std::fs::read_to_string(keys).map_err(|e| format!("{keys}: {e}"))?;
            let keys = vocab::keys_from_json(&json).map_err(|e| format!("{keys}: {e}"))?;
            let package = vocab::package(&format!("The `{name}` cue vocabulary"), &keys)?;
            check_vocab(name, &package, &a.packages)?;
            write(&a.out, &package)
        }
        _ => Err(String::new()),
    }
}

/// Check a generated vocabulary as a presentation imports it, so a function that collides with a name of `std`,
/// `content`, `present`, or another package is reported now.
fn check_vocab(name: &str, package: &str, packages: &[(String, String)]) -> Result<(), String> {
    let mut all = packages.to_vec();
    all.push((name.to_string(), package.to_string()));
    let imports: String = packages.iter().map(|(n, _)| format!("$[Import* {n}]")).collect();
    let probe = format!("$[Import* std]$[Import* content]$[Import* present]{imports}$[Import* {name}]\n");
    let mut engine = lamlet::embed::Engine::new();
    engine.set_resolver(lamlet_present::resolver(&all));
    engine.check(SourceFile::new("<vocabulary check>", probe)).map(|_| ()).map_err(|e| e.render())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.is_empty() => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("{}", e.trim_end());
            ExitCode::FAILURE
        }
    }
}
