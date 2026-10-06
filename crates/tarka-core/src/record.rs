//! One input record, as every source delivers it.

/// A row (or one part of a split row) with a cell per column. A cell is `None` when
/// it is unbound (empty, unless empty strings are bound).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// The row number, from 0. The parts of a split row share it.
    pub row: u64,
    pub cells: Vec<Option<String>>,
}
