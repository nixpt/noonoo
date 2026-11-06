//! The `noonoo` command line.

use std::process::ExitCode;

use noonoo::cli::{Command, parse};

const USAGE: &str = "\
noonoo — a modular HTML/CSS rendering engine

USAGE:
    noonoo <COMMAND>

COMMANDS:
    -h, --help       Show this help
    -V, --version    Show the version

The render and headless-dump commands arrive with the slices that implement
them; see docs/ROADMAP.md.
";

fn main() -> ExitCode {
    match parse(std::env::args()) {
        Ok(Command::Version) => {
            println!("noonoo {}", noonoo::VERSION);
            ExitCode::SUCCESS
        }
        Ok(Command::Help) => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("noonoo: {error}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
