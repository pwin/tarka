"""Runs a SPARQL query over RDF files, with the trail's reference data, in a HOLOS store.

    python tools/sparql.py QUERY.rq FILE.ttl [FILE ...] [--no-reference] [--expect-none]

Prints the rows of a SELECT (and how many), the answer to an ASK, or the triples of a
CONSTRUCT. The reference data (vocabulary, genres, places, people) is loaded too, unless
--no-reference is given: a query about where shops are needs to know what the places are.
With --expect-none, a SELECT that gives rows, or an ASK that answers yes, exits with status 1:
for a query whose every row is a fault, in a pipeline.
"""

import sys
from pathlib import Path

import holosdb

HERE = Path(__file__).resolve().parents[1]
REFERENCE = ["01-vocabulary.ttl", "02-genres.ttl", "03-places.ttl", "05-people.ttl"]


def short(term: str | None) -> str:
    if term is None:
        return ""
    if term.endswith("^^<http://www.w3.org/2001/XMLSchema#integer>"):
        return term[1 : term.rindex('"')]  # a count, as a number
    for prefix, ns in [("bt:", "https://example.org/bookshop-trail/"), ("bs:", "https://example.org/bookshop-trail/schema#")]:
        if term.startswith(f"<{ns}") and "/" not in term[len(ns) + 1 : -1] and "#" not in term[len(ns) + 1 : -1]:
            return prefix + term[len(ns) + 1 : -1]
    return term


def main(args: list[str]) -> int:
    reference = "--no-reference" not in args
    expect_none = "--expect-none" in args
    query, *files = [a for a in args if a not in ("--no-reference", "--expect-none")]
    store = holosdb.Store()
    if reference:
        for name in REFERENCE:
            store.load(str(HERE / "reference" / name))
    for f in files:
        store.load(f)
    result = store.query(Path(query).read_text(encoding="utf-8"))
    if isinstance(result, bool):
        print(f"ASK: {'yes' if result else 'no'}")
        return 1 if expect_none and result else 0
    elif result and isinstance(result[0], str):
        for triple in result:
            print(triple, ".")
        print(f"{len(result)} triples")
    else:
        for row in result:
            print("  ".join(short(row.term(v)) for v in row.variables))
        print(f"{len(result)} rows")
        return 1 if expect_none and result else 0
    return 0


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    sys.exit(main(sys.argv[1:]))
