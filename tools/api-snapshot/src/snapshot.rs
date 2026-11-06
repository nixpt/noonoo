//! `api-snapshot <rustdoc.json> <surface> <version> [--out <file>]`
//!
//! Convert a rustdoc-JSON document into an api-drift snapshot file. The
//! `api-drift` CLI has no capture subcommand, so this is that missing piece.
//!
//! Produce the input with a pinned nightly:
//!
//! ```sh
//! cargo +nightly-2026-08-28 rustdoc -p <crate> -- \
//!     -Z unstable-options --output-format json
//! ```

use std::path::PathBuf;

use api_drift::{parse_snapshot_file, render_snapshot_file, to_string_pretty};
use api_drift::producer::snapshot_from_rustdoc_json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("usage: api-snapshot <rustdoc.json> <surface> <version> [--out <file>]");
        std::process::exit(2);
    }
    let (json, surface, version) = (&args[0], &args[1], &args[2]);

    let out: PathBuf = match args.iter().position(|a| a == "--out") {
        Some(i) => PathBuf::from(args.get(i + 1).expect("--out needs a value")),
        None => PathBuf::from(format!("{surface}.snapshot.json")),
    };

    let snapshot = snapshot_from_rustdoc_json(std::path::Path::new(json), version)
        .map_err(|e| format!("{e}"))?;
    let file = render_snapshot_file(surface, &snapshot);
    let text = to_string_pretty(&file)?;

    // Round-trip through the verifier: never commit a file we cannot re-read.
    let _ = parse_snapshot_file(&text)?;

    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&out, text)?;

    println!(
        "{surface}: {} items, {} -> {}",
        snapshot.len(),
        file.content_hash,
        out.display()
    );
    Ok(())
}
