"""Compares RDF that tarka made with the course's own graph (reference/*.ttl), value for value.

    python tools/compare.py out/bookshops.ttl [out/stocks.ttl ...]

Three counts, all zero when the mapping reproduces the course data, and a fourth for information:
  made, not in the course:   a triple tarka made that the course does not have
  in the course, not made:   a value the course gives a subject and property tarka writes
  subjects missed:           a subject of a class tarka writes that tarka did not make

Literals are compared by value: "80"^^xsd:decimal equals "80.0"^^xsd:decimal, as SPARQL's
`=` has it, and WKT literals as geometries (geof:sfEquals, from HOLOS's GeoSPARQL functions).
A title the course writes with an RDF 1.2 base direction ("…"@ar--rtl) is counted apart:
tarka writes RDF 1.1, so its title has the language and not the direction.
Exits with status 1 when any of the three is not zero.
"""

import sys
from pathlib import Path

import holosdb

HERE = Path(__file__).resolve().parents[1]

SAME = """(sameTerm(?o, ?o2) || (isLiteral(?o) && isLiteral(?o2) && datatype(?o) = datatype(?o2)
          && lang(?o) = lang(?o2) && (?o = ?o2 || (datatype(?o) = geo:wktLiteral && geof:sfEquals(?o, ?o2)))))"""
# the same text and language, the course's with an RDF 1.2 base direction (@ar--rtl)
DIRECTION = """(isLiteral(?o) && isLiteral(?o2) && STR(?o) = STR(?o2) && LANG(?o) = LANG(?o2)
          && (LANGDIR(?o) != "" || LANGDIR(?o2) != ""))"""
PREFIXES = """PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
PREFIX geo: <http://www.opengis.net/ont/geosparql#>
PREFIX geof: <http://www.opengis.net/def/function/geosparql/>
"""

QUERIES = {
    "made, not in the course": f"""
        SELECT ?s ?p ?o WHERE {{ GRAPH <urn:made> {{ ?s ?p ?o }}
          FILTER NOT EXISTS {{ GRAPH <urn:course> {{ ?s ?p ?o2 }} FILTER ({SAME} || {DIRECTION}) }} }}""",
    "in the course, not made": f"""
        SELECT ?s ?p ?o WHERE {{ GRAPH <urn:course> {{ ?s ?p ?o }}
          FILTER EXISTS {{ GRAPH <urn:made> {{ ?s ?p ?any }} }}
          FILTER NOT EXISTS {{ GRAPH <urn:made> {{ ?s ?p ?o2 }} FILTER ({SAME} || {DIRECTION}) }} }}""",
    "subjects missed": """
        SELECT ?s ?p ?o WHERE { GRAPH <urn:course> { ?s a ?o } BIND(rdf:type AS ?p)
          FILTER(STRSTARTS(STR(?o), "https://example.org/bookshop-trail/schema#"))
          FILTER EXISTS { GRAPH <urn:made> { ?x a ?o } }
          FILTER NOT EXISTS { GRAPH <urn:made> { ?s ?anything ?y } } }""",
    "base direction (RDF 1.2), which tarka does not write": f"""
        SELECT ?s ?p ?o WHERE {{ GRAPH <urn:course> {{ ?s ?p ?o }} FILTER(LANGDIR(?o) != "")
          FILTER EXISTS {{ GRAPH <urn:made> {{ ?s ?p ?o2 }} FILTER {DIRECTION} }} }}""",
}
INFORMATION = {"base direction (RDF 1.2), which tarka does not write"}


def main(files: list[str]) -> int:
    store = holosdb.Store()
    for f in sorted((HERE / "reference").glob("0*.ttl")):
        store.load(str(f), graph="urn:course")
    for f in files:
        store.load(f, graph="urn:made")
    made = store.query("SELECT (COUNT(*) AS ?n) WHERE { GRAPH <urn:made> { ?s ?p ?o } }")[0].to_dict()["n"]
    print(f"{made} triples made")
    failed = False
    for what, query in QUERIES.items():
        rows = store.query(PREFIXES + query)
        print(f"{what}: {len(rows)}")
        for r in rows[:10]:
            print("   ", r.term("s"), r.term("p"), r.term("o"))
        failed |= bool(rows) and what not in INFORMATION
    return 1 if failed else 0


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    sys.exit(main(sys.argv[1:]))
