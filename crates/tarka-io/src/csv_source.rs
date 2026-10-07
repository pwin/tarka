//! CSV input, read the way oxi-gen reads it.
//!
//! * A quoted field may use `\` (the escape character) before any character, and
//!   `""` for a quote.
//! * Header names are trimmed and lose their quote characters; with `normalize`
//!   they are upper-cased. Without a header row, columns are named `a`…`z`, `A`…`Z`.
//! * A cell that is empty or only whitespace is unbound, unless empty strings are to
//!   be bound.
//! * A split (`--split ORIGINAL NAME SEPARATOR`) repeats the row once per part of a
//!   cell, with the part in an extra column `NAME`. Several splits multiply.

use std::collections::VecDeque;
use std::io::Read;

use tarka_core::{Cell, Record};
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct CsvOptions {
    pub delimiter: u8,
    pub quote: u8,
    pub escape: Option<u8>,
    pub has_headers: bool,
    /// Upper-case the header names.
    pub normalize_headers: bool,
    /// Bind empty (and whitespace-only) cells as empty strings instead of leaving
    /// them unbound.
    pub bind_empty_strings: bool,
    pub splits: Vec<Split>,
    /// Read at most this many rows.
    pub limit: Option<u64>,
}

impl Default for CsvOptions {
    fn default() -> Self {
        Self {
            delimiter: b',',
            quote: b'"',
            escape: Some(b'\\'),
            has_headers: true,
            normalize_headers: false,
            bind_empty_strings: false,
            splits: Vec::new(),
            limit: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Split {
    pub column: String,
    pub name: String,
    pub separator: String,
}

#[derive(Debug, Error)]
pub enum CsvError {
    #[error("row {row}: {source}")]
    Row { row: u64, source: csv::Error },
    #[error("cannot read the header row: {0}")]
    Header(csv::Error),
}

/// Reads records from CSV text.
pub struct CsvSource<R: Read> {
    reader: csv::Reader<R>,
    columns: Vec<String>,
    options: CsvOptions,
    /// For each split: the index of the column it splits.
    split_columns: Vec<Option<usize>>,
    row: u64,
    pending: VecDeque<Record>,
    record: csv::StringRecord,
}

impl<R: Read> CsvSource<R> {
    pub fn new(input: R, options: CsvOptions) -> Result<Self, CsvError> {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(options.has_headers)
            .delimiter(options.delimiter)
            .quote(options.quote)
            .escape(options.escape)
            .from_reader(input);
        let mut columns: Vec<String> = if options.has_headers {
            let header = reader.headers().map_err(CsvError::Header)?;
            header.iter().map(|h| clean_header(h, options.normalize_headers)).collect()
        } else {
            ('a'..='z').chain('A'..='Z').map(String::from).collect()
        };
        if let Some(first) = columns.first_mut() {
            *first = first.trim_start_matches('\u{feff}').to_owned();
        }
        let split_columns = options.splits.iter().map(|s| columns.iter().position(|c| *c == s.column)).collect();
        columns.extend(options.splits.iter().map(|s| s.name.clone()));
        Ok(Self { reader, columns, options, split_columns, row: 0, pending: VecDeque::new(), record: csv::StringRecord::new() })
    }

    /// The column names: the CSV's own, then one per split.
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    fn cell(&self, text: &str) -> Option<Cell> {
        if self.options.bind_empty_strings || !text.trim().is_empty() { Some(Cell::Text(text.to_owned())) } else { None }
    }

    fn read_row(&mut self) -> Option<Result<(), CsvError>> {
        if self.options.limit.is_some_and(|n| self.row >= n) {
            return None;
        }
        match self.reader.read_record(&mut self.record) {
            Ok(false) => None,
            Err(source) => Some(Err(CsvError::Row { row: self.row, source })),
            Ok(true) => {
                let own = self.columns.len() - self.options.splits.len();
                let mut texts: Vec<&str> = self.record.iter().collect();
                if !self.options.has_headers
                    && self.row == 0
                    && let Some(first) = texts.first_mut()
                {
                    *first = first.trim_start_matches('\u{feff}');
                }
                texts.resize(own.max(texts.len()), "");
                texts.truncate(own);
                let base: Vec<Option<Cell>> = texts.iter().map(|t| self.cell(t)).collect();
                let mut records = vec![base];
                for (split, column) in self.options.splits.iter().zip(&self.split_columns) {
                    let Some(column) = column else {
                        // the column does not exist: the split adds nothing
                        records.iter_mut().for_each(|r| r.push(None));
                        continue;
                    };
                    let text = texts[*column];
                    let parts: Vec<Option<Cell>> = text.split(split.separator.as_str()).map(|p| self.cell(p)).collect();
                    records = records
                        .into_iter()
                        .flat_map(|r| {
                            parts.iter().map(move |p| {
                                let mut next = r.clone();
                                next.push(p.clone());
                                next
                            })
                        })
                        .collect();
                }
                let row = self.row;
                self.pending.extend(records.into_iter().map(|cells| Record { row, cells }));
                self.row += 1;
                Some(Ok(()))
            }
        }
    }
}

impl<R: Read> Iterator for CsvSource<R> {
    type Item = Result<Record, CsvError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(r) = self.pending.pop_front() {
                return Some(Ok(r));
            }
            if let Err(e) = self.read_row()? {
                return Some(Err(e));
            }
        }
    }
}

fn clean_header(name: &str, normalize: bool) -> String {
    let name = name.trim().replace('"', "");
    if normalize { name.to_uppercase() } else { name }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str, options: CsvOptions) -> (Vec<String>, Vec<Record>) {
        let source = CsvSource::new(text.as_bytes(), options).unwrap();
        let columns = source.columns().to_vec();
        (columns, source.map(Result::unwrap).collect())
    }

    fn cells(r: &Record) -> Vec<Option<&str>> {
        r.cells.iter().map(|c| c.as_ref().and_then(Cell::text)).collect()
    }

    #[test]
    fn oxigen_dialect() {
        let text = "\u{feff} id ,\"name\"\nex:a,\"x \\\"q\\\" \"\"d\"\", y\nz\"\n ex:b ,   \n";
        let (columns, rows) = read(text, CsvOptions::default());
        assert_eq!(columns, ["id", "name"]);
        assert_eq!(cells(&rows[0]), [Some("ex:a"), Some("x \"q\" \"d\", y\nz")]);
        assert_eq!(cells(&rows[1]), [Some(" ex:b "), None]);
        assert_eq!(rows[1].row, 1);
        let (_, rows) = read(text, CsvOptions { bind_empty_strings: true, ..CsvOptions::default() });
        assert_eq!(cells(&rows[1]), [Some(" ex:b "), Some("   ")]);
    }

    #[test]
    fn splits_multiply_rows() {
        let options = CsvOptions {
            splits: vec![
                Split { column: "a".into(), name: "a1".into(), separator: ";".into() },
                Split { column: "b".into(), name: "b1".into(), separator: "|".into() },
            ],
            ..CsvOptions::default()
        };
        let (columns, rows) = read("a,b\nx;y,1|2\n", options);
        assert_eq!(columns, ["a", "b", "a1", "b1"]);
        assert_eq!(rows.len(), 4);
        assert_eq!(cells(&rows[3]), [Some("x;y"), Some("1|2"), Some("y"), Some("2")]);
        assert!(rows.iter().all(|r| r.row == 0));
    }

    #[test]
    fn no_header_row_and_limits() {
        let options = CsvOptions { has_headers: false, normalize_headers: true, limit: Some(1), ..CsvOptions::default() };
        let (columns, rows) = read("1,2\n3,4\n", options);
        assert_eq!(&columns[..3], ["a", "b", "c"]);
        assert_eq!(rows.len(), 1);
        assert_eq!(cells(&rows[0])[..2], [Some("1"), Some("2")]);
        let (columns, _) = read("Name\nx\n", CsvOptions { normalize_headers: true, ..CsvOptions::default() });
        assert_eq!(columns, ["NAME"]);
    }

    #[test]
    fn ragged_rows_are_errors() {
        let source = CsvSource::new("a,b\n1\n".as_bytes(), CsvOptions::default()).unwrap();
        let err = source.into_iter().next().unwrap().unwrap_err();
        assert!(err.to_string().starts_with("row 0:"), "{err}");
    }
}
