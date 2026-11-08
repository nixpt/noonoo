//! The document mode from the doctype: HTML's "initial" insertion mode.

use noonoo_dom::QuirksMode;

use crate::Token;

/// Public identifiers that put a document in quirks mode, matched as
/// case-insensitive prefixes.
const QUIRKS_PREFIXES: &[&str] = &[
    "+//silmaril//dtd html pro v0r11 19970101//",
    "-//as//dtd html 3.0 aswedit + extensions//",
    "-//advasoft ltd//dtd html 3.0 aswedit + extensions//",
    "-//ietf//dtd html 2.0 level 1//",
    "-//ietf//dtd html 2.0 level 2//",
    "-//ietf//dtd html 2.0 strict level 1//",
    "-//ietf//dtd html 2.0 strict level 2//",
    "-//ietf//dtd html 2.0 strict//",
    "-//ietf//dtd html 2.0//",
    "-//ietf//dtd html 2.1e//",
    "-//ietf//dtd html 3.0//",
    "-//ietf//dtd html 3.2 final//",
    "-//ietf//dtd html 3.2//",
    "-//ietf//dtd html 3//",
    "-//ietf//dtd html level 0//",
    "-//ietf//dtd html level 1//",
    "-//ietf//dtd html level 2//",
    "-//ietf//dtd html level 3//",
    "-//ietf//dtd html strict level 0//",
    "-//ietf//dtd html strict level 1//",
    "-//ietf//dtd html strict level 2//",
    "-//ietf//dtd html strict level 3//",
    "-//ietf//dtd html strict//",
    "-//ietf//dtd html//",
    "-//metrius//dtd metrius presentational//",
    "-//microsoft//dtd internet explorer 2.0 html strict//",
    "-//microsoft//dtd internet explorer 2.0 html//",
    "-//microsoft//dtd internet explorer 2.0 tables//",
    "-//microsoft//dtd internet explorer 3.0 html strict//",
    "-//microsoft//dtd internet explorer 3.0 html//",
    "-//microsoft//dtd internet explorer 3.0 tables//",
    "-//netscape comm. corp.//dtd html//",
    "-//netscape comm. corp.//dtd strict html//",
    "-//o'reilly and associates//dtd html 2.0//",
    "-//o'reilly and associates//dtd html extended 1.0//",
    "-//o'reilly and associates//dtd html extended relaxed 1.0//",
    "-//sq//dtd html 2.0 hotmetal + extensions//",
    "-//softquad software//dtd hotmetal pro 6.0::19990601::extensions to html 4.0//",
    "-//softquad//dtd hotmetal pro 4.0::19971010::extensions to html 4.0//",
    "-//spyglass//dtd html 2.0 extended//",
    "-//sun microsystems corp.//dtd hotjava html//",
    "-//sun microsystems corp.//dtd hotjava strict html//",
    "-//w3c//dtd html 3 1995-03-24//",
    "-//w3c//dtd html 3.2 draft//",
    "-//w3c//dtd html 3.2 final//",
    "-//w3c//dtd html 3.2//",
    "-//w3c//dtd html 3.2s draft//",
    "-//w3c//dtd html 4.0 frameset//",
    "-//w3c//dtd html 4.0 transitional//",
    "-//w3c//dtd html experimental 19960712//",
    "-//w3c//dtd html experimental 970421//",
    "-//w3c//dtd w3 html//",
    "-//w3o//dtd w3 html 3.0//",
    "-//webtechs//dtd mozilla html 2.0//",
    "-//webtechs//dtd mozilla html//",
];

/// Public identifiers that are quirks as a whole.
const QUIRKS_EXACT: &[&str] = &[
    "-//w3o//dtd w3 html strict 3.0//en//",
    "-/w3c/dtd html 4.0 transitional/en",
    "html",
];

/// HTML 4.01's loose doctypes: quirks without a system identifier,
/// limited-quirks with one.
const HTML401_LOOSE: &[&str] = &[
    "-//w3c//dtd html 4.01 frameset//",
    "-//w3c//dtd html 4.01 transitional//",
];

/// XHTML 1.0's loose doctypes: always limited-quirks.
const XHTML_LOOSE: &[&str] = &[
    "-//w3c//dtd xhtml 1.0 frameset//",
    "-//w3c//dtd xhtml 1.0 transitional//",
];

/// The mode `tokens` put a parsed document in: the doctype's, when one
/// comes before anything but comments and white space; quirks without.
pub(crate) fn of(tokens: &[Token]) -> QuirksMode {
    for token in tokens {
        match token {
            Token::Comment(_) => {}
            Token::Text(text) if text.trim().is_empty() => {}
            Token::Doctype {
                name,
                public_id,
                system_id,
            } => return of_doctype(name, public_id.as_deref(), system_id.as_deref()),
            _ => return QuirksMode::Quirks,
        }
    }
    QuirksMode::Quirks
}

fn of_doctype(name: &str, public_id: Option<&str>, system_id: Option<&str>) -> QuirksMode {
    if !name.eq_ignore_ascii_case("html") {
        return QuirksMode::Quirks;
    }
    let public = public_id.map(str::to_ascii_lowercase);
    let public = public.as_deref();
    let starts =
        |prefixes: &[&str]| public.is_some_and(|id| prefixes.iter().any(|p| id.starts_with(p)));
    if public.is_some_and(|id| QUIRKS_EXACT.contains(&id))
        || system_id.is_some_and(|id| {
            id.eq_ignore_ascii_case("http://www.ibm.com/data/dtd/v11/ibmxhtml1-transitional.dtd")
        })
        || starts(QUIRKS_PREFIXES)
        || (system_id.is_none() && starts(HTML401_LOOSE))
    {
        return QuirksMode::Quirks;
    }
    if starts(XHTML_LOOSE) || (system_id.is_some() && starts(HTML401_LOOSE)) {
        return QuirksMode::LimitedQuirks;
    }
    QuirksMode::NoQuirks
}
