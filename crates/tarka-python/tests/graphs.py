"""RDF graphs as sets of triples whose terms are in N-Triples form, read with HOLOS (holosdb),
and compared with blank nodes matched by structure.

Lexical forms are kept as written: "01"^^xsd:integer is not "1"^^xsd:integer. A blank node is
named by its place in the graph: colour refinement over its edges, then, where nodes are still
alike, each set apart in turn and the least result kept. Two graphs are isomorphic exactly
when their canonical forms are equal. The search is meant for graphs of fixture size, not for
millions of triples.
"""

import hashlib
import os
import tempfile
from collections import defaultdict

import holosdb

Triple = tuple[str, str, str]
Edges = dict[str, list[tuple[str, str, str]]]


def read(path, fmt: str | None = None) -> set[Triple]:
    """The triples of an RDF file; the format comes from its extension unless `fmt` is given
    (`turtle` or `ntriples`)."""
    store = holosdb.Store()
    store.load(str(path), format=fmt)
    query = "SELECT ?s ?p ?o WHERE { ?s ?p ?o }"
    return {(row.term("s"), row.term("p"), row.term("o")) for row in store.query(query)}


def parse(text: str, fmt: str) -> set[Triple]:
    """The triples of RDF text in `fmt` (`turtle` or `ntriples`)."""
    fd, name = tempfile.mkstemp(suffix=".rdf")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            f.write(text)
        return read(name, fmt)
    finally:
        os.remove(name)


def is_blank(term: str) -> bool:
    return term.startswith("_:")


def canonical(triples: set[Triple]) -> list[Triple]:
    """The triples, sorted, with each blank node named by its place in the graph."""
    edges: Edges = defaultdict(list)
    for s, p, o in triples:
        if is_blank(s):
            edges[s].append((">", p, o))
        if is_blank(o):
            edges[o].append(("<", p, s))
    return _search(triples, edges, _refine(edges, dict.fromkeys(edges, "")))


def difference(want: set[Triple], got: set[Triple]) -> tuple[list[Triple], list[Triple]]:
    """The triples only in `want` and only in `got`, blank nodes named canonically; both are
    empty when the graphs are isomorphic."""
    a, b = set(canonical(want)), set(canonical(got))
    return sorted(a - b), sorted(b - a)


def _hash(*parts: str) -> str:
    return hashlib.sha256("\x00".join(parts).encode()).hexdigest()[:16]


def _refine(edges: Edges, colour: dict[str, str]) -> dict[str, str]:
    """Each blank node's colour joined with its edges', until no class splits."""
    classes = len(set(colour.values()))
    while True:
        old = colour
        node = lambda t: "_:" + old[t] if t in old else t
        colour = {b: _hash(c, *sorted(f"{d} {p} {node(t)}" for d, p, t in edges[b])) for b, c in old.items()}
        # a new colour includes the old one, so the same number of classes is the same classes
        if len(set(colour.values())) == classes:
            return colour
        classes = len(set(colour.values()))


def _search(triples: set[Triple], edges: Edges, colour: dict[str, str]) -> list[Triple]:
    classes = defaultdict(list)
    for b, c in colour.items():
        classes[c].append(b)
    tied = [nodes for nodes in classes.values() if len(nodes) > 1]
    if not tied:
        name = lambda t: "_:b" + colour[t] if t in colour else t
        return sorted((name(s), p, name(o)) for s, p, o in triples)
    # nothing tells these nodes apart yet: set each apart in turn, and keep the least form
    nodes = min(tied, key=lambda ns: (len(ns), colour[ns[0]]))
    return min(_search(triples, edges, _refine(edges, colour | {b: _hash(colour[b], "*")})) for b in _one_of_each_twin(edges, nodes))


def _one_of_each_twin(edges: Edges, nodes: list[str]) -> list[str]:
    """The nodes, less those with the same edges as one before: swapping two such nodes leaves
    the graph as it was, so setting apart one or the other gives the same form."""
    seen, chosen = set(), []
    for b in nodes:
        key = frozenset((d, p, "*" if t == b else t) for d, p, t in edges[b])
        if key not in seen:
            seen.add(key)
            chosen.append(b)
    return chosen
