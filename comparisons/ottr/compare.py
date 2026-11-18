"""Compares two RDF files (N-Triples or Turtle, by extension) as graphs, blank nodes matched
by structure: whether they are isomorphic, whether their triples without blank nodes are the
same, and how many of the retail templates' blank nodes lack the value their template
requires. -v lists the triples that differ. The files are read with HOLOS (holosdb), and
compared with the Python tests' graphs module.

    python comparisons/ottr/compare.py A.nt B.ttl [-v]
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "crates" / "tarka-python" / "tests"))
from graphs import canonical, is_blank, read  # noqa: E402

RDF_TYPE = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>"
S, EX = "https://schema.org/", "http://example.com/ns#"
# a node's type, and the property its template makes mandatory
required = {S + "PriceSpecification": S + "price", EX + "Shipment": EX + "shippedOn", S + "OrderItem": EX + "lineNumber"}


def local(iri: str) -> str:
    return iri.rsplit("/", 1)[-1].rsplit("#", 1)[-1]


a, b = sys.argv[1:3]
graphs = {p: read(p) for p in (a, b)}
print(f"{a}: {len(graphs[a])} triples; {b}: {len(graphs[b])} triples")
forms = {p: canonical(g) for p, g in graphs.items()}
print(f"isomorphic: {forms[a] == forms[b]}")
plain = {p: {(s, pr, o) for s, pr, o in g if not is_blank(s) and not is_blank(o)} for p, g in graphs.items()}
print(f"triples without blank nodes: {len(plain[a])} and {len(plain[b])}, the same: {plain[a] == plain[b]}")
for p, g in graphs.items():
    counts = []
    for cls, prop in required.items():
        nodes = {s for s, pr, o in g if pr == RDF_TYPE and o == f"<{cls}>"}
        missing = len(nodes - {s for s, pr, _ in g if pr == f"<{prop}>"})
        counts.append(f"{local(cls)} {len(nodes)} ({missing} without {local(prop)})")
    print(f"{p}: " + ", ".join(counts))
if "-v" in sys.argv[3:]:
    only = {a: set(forms[a]) - set(forms[b]), b: set(forms[b]) - set(forms[a])}
    for p, triples in only.items():
        print(f"only in {p}: {len(triples)}")
        for s, pr, o in sorted(triples):
            print(f"    {s} {pr} {o} .")
