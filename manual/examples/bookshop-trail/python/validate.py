"""Validates the transformation's output with SHACL, two ways.

    python python/validate.py out/bookshops.ttl [more files ...]

1. SHACL_Engine (the `shacl` package), with the shapes tarka makes from tarql/bookshops.rq,
   over the first file: does the bookshops' output have the form the mapping says it has?
2. HOLOS (`holosdb`), with the SHACL course's shapes for the whole trail (s64), over every
   file and the reference data: is the output right for the trail? s64's shapes follow a
   shop's town to its country (bs:locatedIn, bs:within), so the places, genres and people
   must be in the data graph as well as what tarka made.

s64 reports a handful of warnings on the course's own data, by design: they are the course's
puzzles (q34's cycle of influence, a memorial event, two shops off the footpaths). What
counts is violations. Exits with status 1 when there is a violation.
"""

import sys
from pathlib import Path

import holosdb
import shacl
import tarka_rdf

HERE = Path(__file__).resolve().parents[1]
REFERENCE = ["01-vocabulary.ttl", "02-genres.ttl", "03-places.ttl", "05-people.ttl"]


def main(files: list[str]) -> int:
    ok = True

    # 1. The mapping's own shapes, checked with SHACL_Engine
    mapping = tarka_rdf.Mapping.tarql_file(HERE / "tarql" / "bookshops.rq")
    shapes = shacl.Shapes.from_turtle(mapping.shapes(base="https://example.org/bookshop-trail/shapes#"))
    report = shapes.validate_turtle(Path(files[0]).read_text(encoding="utf-8"))
    print(f"{Path(files[0]).name} against the mapping's shapes: {'conforms' if report.conforms else 'does not conform'}")
    for r in report.results[:10]:
        print(f"   {r.focus_node} {r.path} {r.component}")
    ok &= report.conforms

    # 2. The SHACL course's shapes, with the reference data, in a HOLOS store
    store = holosdb.Store()
    for name in REFERENCE:
        store.load(str(HERE / "reference" / name))
    for f in files:
        store.load(f)
    report = store.validate(str(HERE / "reference" / "s64-the-whole-trail-in-one-shapes-graph.ttl"))
    results = sorted(report["results"], key=lambda r: (r["severity"] != "Violation", str(r["focus"]), str(r["path"])))
    violations = [r for r in results if r["severity"] == "Violation"]
    print(f"everything against s64, with the reference data: {len(violations)} violations, "
          f"{len(results) - len(violations)} warnings")
    for r in results:
        print(f"   {r['severity']}: {r['focus']} {r['path'] or ''} {r['component']}: {r['message'] or ''}")
    ok &= not violations
    return 0 if ok else 1


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    sys.exit(main(sys.argv[1:]))
