//! The `vaji` command line.

use std::process::ExitCode;

use vaji::cli::{Command, parse};

const USAGE: &str = "\
vaji — a modular HTML/CSS rendering engine

USAGE:
    vaji <COMMAND>

COMMANDS:
    -h, --help       Show this help
    -V, --version    Show the version

The render and headless-dump commands arrive with the slices that implement
them; see docs/ROADMAP.md.
";

fn main() -> ExitCode {
    match parse(std::env::args()) {
        Ok(Command::Version) => {
            println!("vaji {}", vaji::VERSION);
            ExitCode::SUCCESS
        }
        Ok(Command::Help) => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("vaji: {error}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
