//! # noonoo-csv
//!
//! A dependency-free CSV/TSV parser and an HTML-table adapter. Like Markdown,
//! this is a *viewer*: parse the data, emit a `<table>`, and let the engine lay
//! it out and paint it (`docs/FORMATS.md`).
//!
//! Parsing follows RFC 4180 where it matters — quoted fields, embedded
//! delimiters and newlines, and doubled quotes inside quotes — and the
//! delimiter (`,` or tab) is detected, so the same path handles `.csv` and
//! `.tsv`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Parse CSV (`comma`-delimited) text into rows of fields.
pub fn parse_csv(source: &str) -> Vec<Vec<String>> {
    parse(source, ',')
}

/// Parse TSV (tab-delimited) text into rows of fields.
pub fn parse_tsv(source: &str) -> Vec<Vec<String>> {
    parse(source, '\t')
}

/// Guess the delimiter: tab if the first line has more tabs than commas, else comma.
pub fn detect_delimiter(source: &str) -> char {
    let first_line = source.lines().next().unwrap_or("");
    let tabs = first_line.matches('\t').count();
    let commas = first_line.matches(',').count();
    if tabs > commas { '\t' } else { ',' }
}

/// Parse `source` with an explicit delimiter.
pub fn parse(source: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut saw_input = false;

    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        saw_input = true;
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(ch);
            }
            continue;
        }

        match ch {
            '"' if field.is_empty() => in_quotes = true,
            c if c == delimiter => row.push(std::mem::take(&mut field)),
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            other => field.push(other),
        }
    }

    if saw_input && (!field.is_empty() || !row.is_empty()) {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// Convert delimited text to an HTML `<table>` (first row is the header).
pub fn to_html(source: &str) -> String {
    let delimiter = detect_delimiter(source);
    let rows = parse(source, delimiter);
    if rows.is_empty() {
        return String::new();
    }

    let mut out = String::from("<table><thead><tr>");
    for cell in &rows[0] {
        out.push_str("<th>");
        out.push_str(&escape(cell));
        out.push_str("</th>");
    }
    out.push_str("</tr></thead><tbody>");
    for row in &rows[1..] {
        out.push_str("<tr>");
        for cell in row {
            out.push_str("<td>");
            out.push_str(&escape(cell));
            out.push_str("</td>");
        }
        out.push_str("</tr>");
    }
    out.push_str("</tbody></table>");
    out
}

/// A small stylesheet tuned for the engine's block/inline layout (Noonoo has no
/// CSS table model yet, so rows are blocks and cells inline boxes).
pub const DEFAULT_STYLESHEET: &str = r#"
body { margin: 16px; color: #1f2328; background-color: #ffffff }
thead, tbody { display: block }
tr { display: block; padding: 4px 0; border-bottom: 1px solid #d0d7de }
th, td { display: inline-block; padding: 2px 10px; font-size: 14px }
th { font-weight: bold }
"#;

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_rows() {
        assert_eq!(parse_csv("a,b\n1,2"), vec![vec!["a", "b"], vec!["1", "2"]]);
    }

    #[test]
    fn handles_quotes_and_delimiters_inside_fields() {
        assert_eq!(
            parse_csv("\"a,b\",c\n\"line\nbreak\",d"),
            vec![
                vec!["a,b".to_string(), "c".to_string()],
                vec!["line\nbreak".to_string(), "d".to_string()],
            ]
        );
    }

    #[test]
    fn handles_doubled_quotes() {
        assert_eq!(
            parse_csv("\"she said \"\"hi\"\"\""),
            vec![vec!["she said \"hi\""]]
        );
    }

    #[test]
    fn detects_tabs() {
        assert_eq!(detect_delimiter("a\tb\tc\n1\t2\t3"), '\t');
        assert_eq!(detect_delimiter("a,b,c"), ',');
        assert_eq!(parse_tsv("a\tb"), vec![vec!["a", "b"]]);
    }

    #[test]
    fn builds_a_table() {
        let html = to_html("name,score\nada,10\nalan,7");
        assert!(html.starts_with("<table><thead><tr><th>name</th><th>score</th></tr></thead>"));
        assert!(html.contains("<tr><td>ada</td><td>10</td></tr>"));
        assert!(html.ends_with("</tbody></table>"));
    }

    #[test]
    fn escapes_cell_html() {
        assert!(to_html("a\n<b>").contains("<td>&lt;b&gt;</td>"));
    }
}
