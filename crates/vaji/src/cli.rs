//! Command-line parsing for the `vaji` binary.
//!
//! Kept apart from `main` so it can be tested without spawning a process.

/// What the user asked the binary to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Print help and exit.
    Help,
    /// Print the version and exit.
    Version,
}

/// Parse an argument list (including the program name, as `std::env::args`
/// yields it) into a [`Command`].
///
/// The vocabulary is small today on purpose: the render/dump commands arrive
/// with the slices that implement them (see `docs/ROADMAP.md`), and a parser
/// that only knows what it can honour is more honest than one full of stubs.
pub fn parse<I, S>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args = args.into_iter();
    let _program = args.next();

    match args.next() {
        None => Ok(Command::Help),
        Some(arg) => match arg.as_ref() {
            "-h" | "--help" => Ok(Command::Help),
            "-V" | "--version" => Ok(Command::Version),
            other => Err(format!("unexpected argument `{other}`")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arguments_shows_help() {
        assert_eq!(parse(["vaji"]), Ok(Command::Help));
    }

    #[test]
    fn version_flags() {
        assert_eq!(parse(["vaji", "--version"]), Ok(Command::Version));
        assert_eq!(parse(["vaji", "-V"]), Ok(Command::Version));
    }

    #[test]
    fn help_flags() {
        assert_eq!(parse(["vaji", "--help"]), Ok(Command::Help));
        assert_eq!(parse(["vaji", "-h"]), Ok(Command::Help));
    }

    #[test]
    fn unknown_argument_is_an_error() {
        assert!(parse(["vaji", "--wat"]).is_err());
    }
}
