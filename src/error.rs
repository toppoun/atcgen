use std::fmt;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Position {
    pub line: usize,
    pub column: usize,
}

/// A DSL error with a one-based line and column.
#[derive(Debug)]
pub struct Error {
    position: Position,
    message: String,
}

impl Error {
    pub(crate) fn at(position: Position, message: impl Into<String>) -> Self {
        Self {
            position,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: {}",
            self.position.line, self.position.column, self.message
        )
    }
}

impl std::error::Error for Error {}
