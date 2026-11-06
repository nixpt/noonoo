//! `api-diff <old.snapshot.json> <new.snapshot.json>`
//!
//! Compare two api-drift snapshots, print each classified break and the
//! suggested fix, and exit non-zero when anything is `Breaking`. There is no
//! `diff` subcommand in the api-drift CLI; this is it.

use api_drift::classify::classify_diff;
use api_drift::diff::diff_snapshots;
use api_drift::suggest::suggest_for_breaks;
use api_drift::{ApiSnapshot, Severity, parse_snapshot_file};

fn load(path: &str) -> Result<ApiSnapshot, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    let file = parse_snapshot_file(&text)?;
    Ok(ApiSnapshot::new(&file.version, file.items))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: api-diff <old.snapshot.json> <new.snapshot.json>");
        std::process::exit(2);
    }

    let old = load(&args[0])?;
    let new = load(&args[1])?;

    let diff = diff_snapshots(&old, &new);
    let breaks = classify_diff(&diff);

    for b in &breaks {
        println!("[{:?}/{:?}] {} — {}", b.severity, b.kind, b.path, b.note);
    }
    for s in suggest_for_breaks(&breaks) {
        println!(
            "suggest {} [{}] auto={}: {}",
            s.path,
            s.action.name(),
            s.auto_appliable,
            s.detail
        );
    }

    let breaking = breaks.iter().any(|b| b.severity == Severity::Breaking);
    println!(
        "{} item(s) changed; {} break(s){}",
        diff.added.len() + diff.removed.len() + diff.changed.len(),
        breaks.len(),
        if breaking { " — BREAKING" } else { "" }
    );

    if breaking {
        std::process::exit(2);
    }
    Ok(())
}
