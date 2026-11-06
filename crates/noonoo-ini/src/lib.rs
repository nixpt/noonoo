//! # noonoo-ini
//!
//! A dependency-free INI parser: `[section]` headers and `key = value` (or
//! `key: value`) pairs, with `;`/`#` comments. It is a **value model** format
//! (`docs/FORMATS.md`): configuration, not markup — so it parses to a typed
//! tree and projects to JSON, sitting beside `noonoo-caison`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use noonoo_json::Json;

/// A parsed INI document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ini {
    /// Sections in order; the global section is named `""`.
    pub sections: Vec<(String, Vec<(String, String)>)>,
}

impl Ini {
    /// The value of `key` in `section`, if present.
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.section(section)?
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// The entries of `section`, if present.
    pub fn section(&self, section: &str) -> Option<&[(String, String)]> {
        self.sections
            .iter()
            .find(|(name, _)| name == section)
            .map(|(_, entries)| entries.as_slice())
    }

    /// Project to JSON: the global section's keys at the top level, then each
    /// named section as a nested object.
    pub fn to_json(&self) -> Json {
        let mut top: Vec<(String, Json)> = Vec::new();
        for (name, entries) in &self.sections {
            if name.is_empty() {
                for (key, value) in entries {
                    top.push((key.clone(), Json::String(value.clone())));
                }
            }
        }
        for (name, entries) in &self.sections {
            if !name.is_empty() {
                top.push((
                    name.clone(),
                    Json::Object(
                        entries
                            .iter()
                            .map(|(k, v)| (k.clone(), Json::String(v.clone())))
                            .collect(),
                    ),
                ));
            }
        }
        Json::Object(top)
    }
}

/// Parse INI text.
pub fn parse(source: &str) -> Ini {
    let mut ini = Ini {
        sections: vec![(String::new(), Vec::new())],
    };
    let mut current = String::new();

    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = name.trim().to_owned();
            if !ini.sections.iter().any(|(s, _)| s == &current) {
                ini.sections.push((current.clone(), Vec::new()));
            }
            continue;
        }

        if let Some((key, value)) = split_pair(line)
            && let Some((_, entries)) = ini.sections.iter_mut().find(|(s, _)| s == &current)
        {
            entries.push((key, value));
        }
    }

    // Drop the global section if it stayed empty.
    if ini
        .sections
        .first()
        .is_some_and(|(_, entries)| entries.is_empty())
    {
        ini.sections.remove(0);
    }
    ini
}

/// Project INI text to JSON.
pub fn to_json(source: &str) -> Json {
    parse(source).to_json()
}

fn split_pair(line: &str) -> Option<(String, String)> {
    let index = line.find('=').or_else(|| line.find(':'))?;
    let key = line[..index].trim();
    let value = line[index + 1..].trim().trim_matches('"');
    (!key.is_empty()).then(|| (key.to_owned(), value.to_owned()))
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
; a comment
name = noonoo

[server]
host: localhost
port = 8080
";

    #[test]
    fn parses_global_and_sections() {
        let ini = parse(SAMPLE);
        assert_eq!(ini.get("", "name"), Some("noonoo"));
        assert_eq!(ini.get("server", "host"), Some("localhost"));
        assert_eq!(ini.get("server", "port"), Some("8080"));
    }

    #[test]
    fn projects_to_json() {
        let json = to_json(SAMPLE).to_string();
        assert!(
            json.starts_with(r#"{"name":"noonoo","server":{"host":"localhost","port":"8080"}}"#)
        );
    }

    #[test]
    fn ignores_comments_and_blanks() {
        let ini = parse("# c\n\n; c\na = 1");
        assert_eq!(ini.get("", "a"), Some("1"));
    }
}
