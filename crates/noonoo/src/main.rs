//! The `noonoo` command line.

use std::io::Read;
use std::process::ExitCode;

use noonoo::cli::{Command, parse};

const USAGE: &str = "\
noonoo — a modular HTML/CSS rendering engine

USAGE:
    noonoo [COMMAND]

COMMANDS:
    -h, --help          Show this help
    -V, --version       Show the version
    --tree <file|->     Parse HTML and print the DOM tree as JSON
    --paint <file|->    Parse HTML, lay out, and print paint commands as JSON
    --md <file|->       Convert Markdown to HTML
    --md-paint <file|-> View Markdown: convert, lay out, and print paint commands
    --caison <file|->   Parse CAISON and print its JSON projection
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
        Ok(Command::Tree { path }) => run_tree(&path),
        Ok(Command::Caison { path }) => run_caison(&path),
        Ok(Command::Paint { path }) => run_paint(&path),
        Ok(Command::Markdown { path }) => run_markdown(&path),
        Ok(Command::MarkdownPaint { path }) => run_markdown_paint(&path),
        Err(error) => {
            eprintln!("noonoo: {error}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Read a source file, or stdin when `path` is `-`.
fn read_source(path: &str) -> Result<String, String> {
    if path == "-" {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .map_err(|error| format!("stdin: {error}"))?;
        Ok(buffer)
    } else {
        std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))
    }
}

fn run_tree(path: &str) -> ExitCode {
    match read_source(path) {
        Ok(html) => {
            let engine = noonoo::engine::Engine::from_html(&html);
            println!("{}", noonoo::headless::tree_json(engine.dom()));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("noonoo: {error}");
            ExitCode::from(1)
        }
    }
}

fn run_caison(path: &str) -> ExitCode {
    let source = match read_source(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("noonoo: {error}");
            return ExitCode::from(1);
        }
    };
    match noonoo::caison::Document::parse(&source).and_then(|doc| doc.to_json()) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("noonoo: {error}");
            ExitCode::from(1)
        }
    }
}

fn run_paint(path: &str) -> ExitCode {
    match read_source(path) {
        Ok(html) => {
            let engine = noonoo::engine::Engine::from_html(&html);
            let list = engine.paint(
                noonoo::geometry::Size::new(800.0, 600.0),
                &noonoo::layout::MetricShaper,
            );
            println!("{}", list.to_json().to_string_pretty());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("noonoo: {error}");
            ExitCode::from(1)
        }
    }
}

fn run_markdown(path: &str) -> ExitCode {
    match read_source(path) {
        Ok(source) => {
            println!("{}", noonoo::markdown::markdown_to_html(&source));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("noonoo: {error}");
            ExitCode::from(1)
        }
    }
}

fn run_markdown_paint(path: &str) -> ExitCode {
    match read_source(path) {
        Ok(source) => {
            // Markdown -> HTML, with a default stylesheet so it renders structured.
            let html = format!(
                "<style>{}</style>{}",
                noonoo::markdown::DEFAULT_STYLESHEET,
                noonoo::markdown::markdown_to_html(&source)
            );
            let engine = noonoo::engine::Engine::from_html(&html);
            let list = engine.paint(
                noonoo::geometry::Size::new(800.0, 600.0),
                &noonoo::layout::MetricShaper,
            );
            println!("{}", list.to_json().to_string_pretty());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("noonoo: {error}");
            ExitCode::from(1)
        }
    }
}
