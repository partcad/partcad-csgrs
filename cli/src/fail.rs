//! Why a request could not be built, in the two kinds the caller treats
//! differently.

use std::fmt;

#[derive(Debug)]
pub enum Fail {
    /// csgrs could not decide a predicate under the strict policy. Worth
    /// asking again under the approximate one, which can decide it.
    Undecided(String),
    /// Anything else: a bad request, a file that cannot be read, geometry that
    /// cannot be built. Asking again changes nothing.
    Other(String),
}

impl Fail {
    pub fn input(message: impl Into<String>) -> Self {
        Fail::Other(message.into())
    }

    /// The same failure, said to be about `what`.
    pub fn context(self, what: &str) -> Self {
        match self {
            Fail::Undecided(m) => Fail::Undecided(format!("{what}: {m}")),
            Fail::Other(m) => Fail::Other(format!("{what}: {m}")),
        }
    }
}

impl fmt::Display for Fail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fail::Undecided(m) | Fail::Other(m) => f.write_str(m),
        }
    }
}

impl From<csgrs::Error> for Fail {
    fn from(e: csgrs::Error) -> Self {
        if e.kind() == csgrs::errors::ValidationErrorKind::Undecided {
            Fail::Undecided(e.to_string())
        } else {
            Fail::Other(e.to_string())
        }
    }
}

/// A failure from below csgrs's own error type (Hypermesh, Hypercurve, I/O):
/// said in words, and classified by what the words say, since those types
/// carry no kind of their own to ask.
pub fn other(e: impl fmt::Display) -> Fail {
    let message = e.to_string();
    if message.to_ascii_lowercase().contains("undecided") {
        Fail::Undecided(message)
    } else {
        Fail::Other(message)
    }
}
