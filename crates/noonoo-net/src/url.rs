//! Resolving a reference (`src="img/a.png"`) against the document's URL.

/// `reference` resolved against `base`, as RFC 3986 §5.2 does for the
/// URLs a document references: a reference with a scheme stands alone; one
/// starting `//` keeps the base's scheme, `/` its authority, `?` its path,
/// `#` its query; anything else replaces the base's last path segment.
/// Dot segments are removed from the result's path.
///
/// A base without a scheme is a filesystem path. A `data:` base has no path,
/// so a relative reference stays as it is.
pub fn resolve(base: &str, reference: &str) -> String {
    if scheme(reference).is_some() {
        return reference.to_owned();
    }
    let base_scheme = scheme(base);
    if base_scheme.is_some_and(|scheme| scheme.eq_ignore_ascii_case("data")) {
        return reference.to_owned();
    }
    let (base_rest, scheme_prefix) = match base_scheme {
        Some(scheme) => (&base[scheme.len() + 1..], &base[..scheme.len() + 1]),
        None => (base, ""),
    };
    if let Some(network) = reference.strip_prefix("//") {
        return format!("{scheme_prefix}//{network}");
    }
    let (authority, rest) = split_authority(base_rest);
    let (path, query) = split_query(rest);
    let prefix = format!("{scheme_prefix}{authority}");
    if reference.is_empty() {
        return format!("{prefix}{path}{query}");
    }
    if reference.starts_with('#') {
        return format!("{prefix}{path}{query}{reference}");
    }
    if reference.starts_with('?') {
        return format!("{prefix}{path}{reference}");
    }
    let joined = if reference.starts_with('/') {
        reference.to_owned()
    } else {
        match path.rfind('/') {
            Some(slash) => format!("{}{reference}", &path[..=slash]),
            // `https://host` has an empty path, which is the root.
            None if !authority.is_empty() => format!("/{reference}"),
            None => reference.to_owned(),
        }
    };
    format!("{prefix}{}", remove_dot_segments(&joined))
}

/// The scheme, if `url` starts with one (`[A-Za-z][A-Za-z0-9+.-]*:`).
fn scheme(url: &str) -> Option<&str> {
    let colon = url.find(':')?;
    let scheme = &url[..colon];
    let mut chars = scheme.chars();
    let first = chars.next()?;
    (first.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-')))
    .then_some(scheme)
}

/// `//authority` and the rest (an empty authority when there's none).
fn split_authority(rest: &str) -> (&str, &str) {
    match rest.strip_prefix("//") {
        Some(after) => {
            let end = after
                .find(['/', '?', '#'])
                .map_or(rest.len(), |end| end + 2);
            (&rest[..end], &rest[end..])
        }
        None => ("", rest),
    }
}

/// The path, and the query with its `?` (the fragment dropped).
fn split_query(rest: &str) -> (&str, &str) {
    let rest = rest.split('#').next().unwrap_or_default();
    match rest.find('?') {
        Some(at) => (&rest[..at], &rest[at..]),
        None => (rest, ""),
    }
}

/// `path` without `.` and `..` segments; `..` never climbs above the root.
fn remove_dot_segments(path: &str) -> String {
    let (path, suffix) = match path.find(['?', '#']) {
        Some(at) => (&path[..at], &path[at..]),
        None => (path, ""),
    };
    let absolute = path.starts_with('/');
    let segments: Vec<&str> = path.split('/').collect();
    let mut out: Vec<&str> = Vec::new();
    for (i, segment) in segments.iter().enumerate() {
        let last = i + 1 == segments.len();
        match *segment {
            "." => {
                if last {
                    out.push("");
                }
            }
            ".." => {
                if out.len() > usize::from(absolute) {
                    out.pop();
                }
                if last {
                    out.push("");
                }
            }
            segment => out.push(segment),
        }
    }
    // An absolute path keeps its leading empty segment, so its slash.
    out.join("/") + suffix
}
