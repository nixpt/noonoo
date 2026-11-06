//! Command-line parsing for the `noonoo` binary.
//!
//! Kept apart from `main` so it can be tested without spawning a process.

/// What the user asked the binary to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Print help and exit.
    Help,
    /// Print the version and exit.
    Version,
    /// Parse an HTML file (or stdin for `-`) and print its DOM tree as JSON.
    Tree {
        /// The input path, or `-` for stdin.
        path: String,
    },
    /// Parse a CAISON file (or stdin for `-`) and print its JSON projection.
    Caison {
        /// The input path, or `-` for stdin.
        path: String,
    },
    /// Parse, style, lay out, and paint an HTML file, printing the command JSON.
    Paint {
        /// The input path, or `-` for stdin.
        path: String,
    },
}

/// Parse an argument list (including the program name, as `std::env::args`
/// yields it) into a [`Command`].
pub fn parse<I, S>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args = args.into_iter().skip(1);
    let Some(first) = args.next() else {
        return Ok(Command::Help);
    };

    match first.as_ref() {
        "-h" | "--help" => Ok(Command::Help),
        "-V" | "--version" => Ok(Command::Version),
        "--tree" => Ok(Command::Tree {
            path: next_or_stdin(&mut args),
        }),
        "--caison" => {
            let path = args
                .next()
                .map(|arg| arg.as_ref().to_owned())
                .ok_or_else(|| "`--caison` needs a file path (or `-`)".to_owned())?;
            Ok(Command::Caison { path })
        }
        "--paint" => Ok(Command::Paint {
            path: next_or_stdin(&mut args),
        }),
        other => Err(format!("unexpected argument `{other}`")),
    }
}

fn next_or_stdin<I, S>(args: &mut I) -> String
where
    I: Iterator<Item = S>,
    S: AsRef<str>,
{
    args.next()
        .map(|arg| arg.as_ref().to_owned())
        .unwrap_or_else(|| "-".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arguments_shows_help() {
        assert_eq!(parse(["noonoo"]), Ok(Command::Help));
    }

    #[test]
    fn version_flags() {
        assert_eq!(parse(["noonoo", "--version"]), Ok(Command::Version));
        assert_eq!(parse(["noonoo", "-V"]), Ok(Command::Version));
    }

    #[test]
    fn tree_reads_a_path_or_stdin() {
        assert_eq!(
            parse(["noonoo", "--tree", "page.html"]),
            Ok(Command::Tree {
                path: "page.html".into()
            })
        );
        assert_eq!(
            parse(["noonoo", "--tree"]),
            Ok(Command::Tree { path: "-".into() })
        );
    }

    #[test]
    fn caison_needs_a_path() {
        assert_eq!(
            parse(["noonoo", "--caison", "app.caison"]),
            Ok(Command::Caison {
                path: "app.caison".into()
            })
        );
        assert!(parse(["noonoo", "--caison"]).is_err());
    }

    #[test]
    fn paint_reads_a_path_or_stdin() {
        assert_eq!(
            parse(["noonoo", "--paint", "page.html"]),
            Ok(Command::Paint {
                path: "page.html".into()
            })
        );
        assert_eq!(
            parse(["noonoo", "--paint"]),
            Ok(Command::Paint { path: "-".into() })
        );
    }

    #[test]
    fn unknown_argument_is_an_error() {
        assert!(parse(["noonoo", "--wat"]).is_err());
    }
}
