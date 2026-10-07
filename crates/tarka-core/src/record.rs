//! One input record, as every source delivers it.

/// A cell: text, as in a CSV file, or a list of texts (a list column of a data frame).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cell {
    Text(String),
    List(Vec<String>),
}

impl Cell {
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(t) => Some(t),
            Self::List(_) => None,
        }
    }
}

impl From<&str> for Cell {
    fn from(s: &str) -> Self {
        Self::Text(s.to_owned())
    }
}

impl From<String> for Cell {
    fn from(s: String) -> Self {
        Self::Text(s)
    }
}

/// A row (or one part of a split row) with a cell per column. A cell is `None` when
/// it is unbound (empty, unless empty strings are bound).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// The row number, from 0. The parts of a split row share it.
    pub row: u64,
    pub cells: Vec<Option<Cell>>,
}
