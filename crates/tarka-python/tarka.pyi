from os import PathLike
from typing import Literal

import polars as pl

__version__: str

_Path = str | PathLike[str]
_Format = Literal["turtle", "ttl", "ntriples", "nt", "nquads", "nq"]

class Mapping:
    """A compiled mapping: a TARQL query or OTTR templates."""

    @staticmethod
    def tarql(query: str, name: str = "query") -> Mapping:
        """A mapping from the text of a TARQL query."""
    @staticmethod
    def tarql_file(path: _Path) -> Mapping:
        """A mapping from a TARQL query file."""
    @staticmethod
    def ottr(library: list[_Path], templates: list[str], lists: dict[str, str] | None = None) -> Mapping:
        """A mapping from OTTR templates: `library` is files or directories of stOTTR, and
        `templates` the root templates to instantiate for every row. `lists` gives the
        separator of text cells for list-typed parameters (list columns need none)."""
    @property
    def name(self) -> str:
        """The plan's name (the query or the root templates)."""
    @property
    def prefixes(self) -> list[tuple[str, str]]:
        """The prefixes, for Turtle output."""
    def triplify(self, df: pl.DataFrame, bind_empty_strings: bool = False) -> pl.DataFrame:
        """The RDF of a data frame, as a data frame of N-Triples terms with the columns
        `subject`, `predicate` and `object` (no duplicates)."""
    def write(
        self,
        df: pl.DataFrame,
        path: _Path | None = None,
        format: _Format = "turtle",
        graph: str | None = None,
        dedup: int = 0,
        bind_empty_strings: bool = False,
    ) -> str | None:
        """Writes the RDF of a data frame to `path`, or returns it as text when `path` is None."""
    def run_csv(
        self,
        input: _Path,
        output: _Path | None = None,
        format: _Format = "turtle",
        graph: str | None = None,
        dedup: int = 0,
        delimiter: str = ",",
        quote: str = '"',
        escape: str = "\\",
        has_header: bool = True,
        bind_empty_strings: bool = False,
        split: list[tuple[str, str, str]] | None = None,
    ) -> str | None:
        """Maps a CSV file (read the way oxi-gen reads it) and writes the RDF to `output`,
        or returns it as text when `output` is None. Each `split` (column, new column,
        separator) repeats a row once per part of the column, as `--split` does."""

def expand(
    library: list[_Path],
    instances: list[_Path],
    output: _Path | None = None,
    format: _Format = "turtle",
    graph: str | None = None,
) -> str | None:
    """Expands OTTR instances (stOTTR files) with a library, as Lutra does; writes to
    `output`, or returns the RDF as text when `output` is None."""
