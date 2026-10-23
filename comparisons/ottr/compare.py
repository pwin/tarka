"""Compares two RDF files (N-Triples or Turtle, by extension) as graphs, blank nodes matched
by structure: whether they are isomorphic, whether their triples without blank nodes are the
same, and how many of the retail templates' blank nodes lack the value their template
requires. -v lists the triples that differ.

    python comparisons/ottr/compare.py A.nt B.ttl [-v]
"""

import sys

from rdflib import RDF, BNode, Graph, URIRef
from rdflib.compare import graph_diff, to_isomorphic
from rdflib.util import guess_format

S, EX = "https://schema.org/", "http://example.com/ns#"
# a node's type, and the property its template makes mandatory
required = {S + "PriceSpecification": S + "price", EX + "Shipment": EX + "shippedOn", S + "OrderItem": EX + "lineNumber"}


def local(iri: str) -> str:
    return iri.rsplit("/", 1)[-1].rsplit("#", 1)[-1]


a, b = sys.argv[1:3]
graphs = {p: Graph().parse(p, format=guess_format(p)) for p in (a, b)}
print(f"{a}: {len(graphs[a])} triples; {b}: {len(graphs[b])} triples")
print(f"isomorphic: {to_isomorphic(graphs[a]) == to_isomorphic(graphs[b])}")
plain = {p: {t for t in g if not any(isinstance(x, BNode) for x in t)} for p, g in graphs.items()}
print(f"triples without blank nodes: {len(plain[a])} and {len(plain[b])}, the same: {plain[a] == plain[b]}")
for p, g in graphs.items():
    counts = []
    for cls, prop in required.items():
        nodes = set(g.subjects(RDF.type, URIRef(cls)))
        missing = sum((n, URIRef(prop), None) not in g for n in nodes)
        counts.append(f"{local(cls)} {len(nodes)} ({missing} without {local(prop)})")
    print(f"{p}: " + ", ".join(counts))
if "-v" in sys.argv[3:]:
    _, only_a, only_b = graph_diff(to_isomorphic(graphs[a]), to_isomorphic(graphs[b]))
    for p, g in [(a, only_a), (b, only_b)]:
        print(f"only in {p}: {len(g)}")
        for line in sorted(g.serialize(format="nt").splitlines()):
            print("   ", line)
