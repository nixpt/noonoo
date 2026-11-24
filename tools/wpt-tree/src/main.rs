//! Run the web-platform tests' tree-construction cases (the html5lib `.dat`
//! format) through noonoo's parser. See README.md.

use noonoo_dom::{Document, Namespace, NodeData, NodeId};
use std::path::{Path, PathBuf};

/// One case of a `.dat` file.
#[derive(Default)]
struct Case {
    data: String,
    /// The context element, for a fragment: `td`, `svg desc`, `math mi`.
    fragment: Option<String>,
    expected: Vec<String>,
    /// Why it isn't run.
    skip: Option<&'static str>,
}

fn main() {
    let (verbose, paths): (Vec<String>, Vec<String>) =
        std::env::args().skip(1).partition(|arg| arg == "-v");
    let mut files = Vec::new();
    for path in &paths {
        let path = Path::new(path);
        if path.is_dir() {
            let entries = std::fs::read_dir(path).expect("the directory is readable");
            let found = entries.flatten().map(|entry| entry.path());
            files.extend(found.filter(|file| file.extension().is_some_and(|e| e == "dat")));
        } else {
            files.push(path.to_owned());
        }
    }
    files.sort();
    if files.is_empty() {
        eprintln!("usage: noonoo-wpt-tree [-v] DIR|FILE.dat ...");
        std::process::exit(2);
    }

    let (mut pass, mut fail, mut skip) = (0, 0, 0);
    for file in &files {
        for case in read(file) {
            if case.skip.is_some() {
                skip += 1;
                continue;
            }
            let got = std::panic::catch_unwind(|| run(&case))
                .unwrap_or_else(|_| vec!["(panicked)".to_owned()]);
            if got == case.expected {
                pass += 1;
                continue;
            }
            fail += 1;
            let name = file.file_name().unwrap_or_default().to_string_lossy();
            println!("FAIL {name}: {:?} in {:?}", case.data, case.fragment);
            if !verbose.is_empty() {
                println!("expected\n{}", case.expected.join("\n"));
                println!("got\n{}", got.join("\n"));
            }
        }
    }
    println!("{pass} pass, {fail} fail, {skip} skipped");
    std::process::exit(i32::from(fail > 0));
}

/// The cases of a file.
fn read(file: &PathBuf) -> Vec<Case> {
    let bytes = std::fs::read(file).expect("the file is readable");
    let text = format!("\n{}", String::from_utf8_lossy(&bytes));
    text.split("\n#data\n")
        .skip(1)
        .map(|source| {
            let mut case = Case::default();
            let (mut data, mut document) = (Vec::new(), Vec::new());
            let mut section = "#data";
            for line in source.split('\n') {
                match line {
                    "#errors" | "#new-errors" | "#description" | "#document-fragment"
                    | "#document" | "#script-on" | "#script-off" => {
                        section = line;
                        if line == "#script-on" {
                            case.skip = Some("scripts don't run while parsing");
                        }
                    }
                    _ => match section {
                        "#data" => data.push(line),
                        "#document-fragment" => case.fragment = Some(line.to_owned()),
                        "#document" => document.push(line),
                        _ => {}
                    },
                }
            }
            while document.last() == Some(&"") {
                document.pop();
            }
            case.data = data.join("\n");
            if document
                .iter()
                .any(|line| line.trim_start_matches(['|', ' ']).starts_with("<?"))
            {
                case.skip = Some("the DOM has no processing instructions");
            }
            case.expected = normalize(&document);
            case
        })
        .collect()
}

/// The expected tree as [`dump`] writes it.
fn normalize(document: &[&str]) -> Vec<String> {
    // Text may go over several lines.
    let mut nodes: Vec<String> = Vec::new();
    for line in document {
        match nodes.last_mut() {
            Some(last) if !line.starts_with("| ") => {
                last.push('\n');
                last.push_str(line);
            }
            _ => nodes.push((*line).to_owned()),
        }
    }
    for node in &mut nodes {
        let content = node.get(2..).unwrap_or("").trim_start().to_owned();
        let indent = node[..node.len() - content.len()].to_owned();
        let content = if content.starts_with("<svg ") || content.starts_with("<math ") {
            content.to_ascii_lowercase()
        } else if let Some(doctype) = content.strip_prefix("<!DOCTYPE") {
            let inner = doctype.trim_end_matches('>').trim_start();
            format!("<!DOCTYPE {}>", inner.split(' ').next().unwrap_or(""))
        } else {
            // `xlink href="..."` is the attribute `xlink:href`.
            ["xlink ", "xml ", "xmlns "]
                .iter()
                .filter(|_| content.contains("=\""))
                .find_map(|prefix| Some((prefix, content.strip_prefix(prefix)?)))
                .map(|(prefix, rest)| format!("{}:{rest}", prefix.trim()))
                .unwrap_or(content)
        };
        *node = indent + &content;
    }
    // Attributes are in name order, by their new names.
    let is_attribute = |line: &String| {
        let content = line.get(2..).unwrap_or("").trim_start();
        !content.starts_with(['<', '"']) && content.contains("=\"")
    };
    let mut from = 0;
    while from < nodes.len() {
        let run = nodes[from..].iter().take_while(|l| is_attribute(l)).count();
        nodes[from..from + run].sort();
        from += run.max(1);
    }
    nodes
}

/// The tree noonoo gives for a case.
fn run(case: &Case) -> Vec<String> {
    let mut out = Vec::new();
    let Some(context) = &case.fragment else {
        let doc = noonoo_html::parse(&case.data);
        dump(&doc, doc.root(), 0, &mut out);
        return out;
    };
    let mut doc = noonoo_html::parse("<!doctype html>");
    let (namespace, name) = match context.split_once(' ') {
        Some(("svg", name)) => (Namespace::Svg, name),
        Some(("math", name)) => (Namespace::MathMl, name),
        _ => (Namespace::Html, context.as_str()),
    };
    let node = doc.create_element_ns(name, namespace);
    noonoo_html::set_inner_html(&mut doc, node, &case.data).expect("a context holds children");
    dump(&doc, node, 0, &mut out);
    out
}

fn dump(doc: &Document, node: NodeId, depth: usize, out: &mut Vec<String>) {
    for &child in doc.tree().children(node) {
        let indent = format!("| {}", "  ".repeat(depth));
        match doc.node(child) {
            Some(NodeData::Text(text)) => out.push(format!("{indent}\"{text}\"")),
            Some(NodeData::Comment(text)) => out.push(format!("{indent}<!-- {text} -->")),
            Some(NodeData::Doctype { name }) => out.push(format!("{indent}<!DOCTYPE {name}>")),
            Some(NodeData::Element(element)) => {
                let namespace = match element.namespace {
                    Namespace::Html => "",
                    Namespace::Svg => "svg ",
                    Namespace::MathMl => "math ",
                };
                let name = element.name.to_ascii_lowercase();
                out.push(format!("{indent}<{namespace}{name}>"));
                let attributes = element.attrs.iter();
                let mut lines: Vec<_> = attributes
                    .map(|attr| format!("{indent}  {}=\"{}\"", attr.name, attr.value))
                    .collect();
                lines.sort();
                out.extend(lines);
                // A template's contents are its children here.
                if namespace.is_empty() && name == "template" {
                    out.push(format!("{indent}  content"));
                    dump(doc, child, depth + 2, out);
                } else {
                    dump(doc, child, depth + 1, out);
                }
            }
            _ => {}
        }
    }
}
