"""The Python module gives the RDF the Rust tests expect, from data frames and CSV files."""

import datetime as dt
from pathlib import Path

import polars as pl
import pytest
import rdflib
from rdflib.compare import graph_diff, isomorphic, to_isomorphic

import tarka

# compare lexical forms as written: "01"^^xsd:integer is not "1"^^xsd:integer
rdflib.NORMALIZE_LITERALS = False

FIXTURES = Path(__file__).resolve().parents[3] / "tests" / "fixtures"
XSD = "http://www.w3.org/2001/XMLSchema#"


def parse(data, fmt):
    return rdflib.Graph().parse(data=data, format=fmt)


def expected(rel):
    return rdflib.Graph().parse(FIXTURES / rel, format="nt")


def frame_graph(triples):
    """The graph of a frame of N-Triples terms."""
    return parse("".join(f"{s} {p} {o} .\n" for s, p, o in triples.iter_rows()), "nt")


def assert_same(want, got, what):
    if not isomorphic(want, got):
        _, only_want, only_got = graph_diff(to_isomorphic(want), to_isomorphic(got))
        lines = lambda g: "\n".join(f"  {s.n3()} {p.n3()} {o.n3()}" for s, p, o in sorted(g))
        pytest.fail(f"{what}: graphs differ\nonly expected:\n{lines(only_want)}\nonly actual:\n{lines(only_got)}")


def text_frame(rel):
    """A CSV fixture as string columns (null for an empty cell)."""
    return pl.read_csv(FIXTURES / rel, infer_schema=False)


def test_tarql_over_a_frame():
    m = tarka.Mapping.tarql_file(FIXTURES / "extra" / "people.rq")
    assert m.name == "people"
    assert ("foaf", "http://xmlns.com/foaf/0.1/") in m.prefixes
    triples = m.triplify(text_frame("extra/people.csv"))
    assert triples.columns == ["subject", "predicate", "object"]
    assert triples.n_unique() == triples.height
    assert_same(expected("extra/expected/people.nt"), frame_graph(triples), "people.rq")


def test_ottr_over_a_frame_with_text_or_list_cells():
    employee = expected("people/expected/employee.nt")
    library = [FIXTURES / "people" / "people.stottr"]
    # a text column split on ";"
    m = tarka.Mapping.ottr(library, ["ex:Employee"], lists={"skills": ";"})
    assert_same(employee, frame_graph(m.triplify(text_frame("people/people.csv"))), "text skills")
    # a list column needs no separator
    df = pl.DataFrame(
        {
            "person": ["ex:alice", "ex:bob", "ex:carol"],
            "name": ["Alice", "Bob", "Carol"],
            "email": ["alice@example.com", None, "carol@example.com"],
            "manager": ["ex:carol", "ex:carol", None],
            "skills": [["python", "rust", "sparql"], ["sparql"], None],
        }
    )
    m = tarka.Mapping.ottr(library, ["ex:Employee"])
    assert_same(employee, frame_graph(m.triplify(df)), "list skills")


def test_tarql_and_ottr_mappings_agree_on_csv():
    for dataset, root in [("customers", "rt:CustomerRow"), ("products", "rt:ProductRow"), ("orders", "rt:OrderRow")]:
        want = expected(f"retail/expected/{dataset}.nt")
        csv = FIXTURES / "retail" / f"{dataset}.csv"
        native_tarql = tarka.Mapping.tarql_file(FIXTURES / "retail" / "tarql" / f"{dataset}.rq")
        native_ottr = tarka.Mapping.ottr([FIXTURES / "retail" / "ottr"], [root])
        for m in (native_tarql, native_ottr):
            assert_same(want, parse(m.run_csv(csv, format="ntriples"), "nt"), f"{dataset} with {m!r}")


def test_typed_columns(tmp_path):
    (tmp_path / "row.stottr").write_text(
        """@prefix ex: <http://example.com/> . @prefix ottr: <http://ns.ottr.xyz/0.4/> .
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
        ex:Row [ ottr:IRI ?id, ? xsd:integer ?n, ? xsd:decimal ?price, ? xsd:boolean ?ok, ? xsd:date ?day,
                 ? xsd:dateTime ?at ] :: {
          ottr:Triple(?id, ex:n, ?n), ottr:Triple(?id, ex:price, ?price), ottr:Triple(?id, ex:ok, ?ok),
          ottr:Triple(?id, ex:day, ?day), ottr:Triple(?id, ex:at, ?at) } .""",
        encoding="utf-8",
    )
    df = pl.DataFrame(
        {
            "id": ["ex:a", "ex:b"],
            "n": [7, None],
            "price": [9.5, 0.25],
            "ok": [True, False],
            "day": [dt.date(2024, 3, 5), dt.date(2024, 12, 31)],
            "at": [dt.datetime(2024, 3, 5, 10, 15), None],
        }
    ).with_columns(pl.col("at").dt.replace_time_zone("UTC"))
    m = tarka.Mapping.ottr([tmp_path], ["http://example.com/Row"])
    got = {(str(s), str(p), o) for s, p, o in frame_graph(m.triplify(df))}
    ex = "http://example.com/"
    lit = lambda v, t: rdflib.Literal(v, datatype=rdflib.URIRef(XSD + t))
    assert got == {
        (ex + "a", ex + "n", lit("7", "integer")),
        (ex + "a", ex + "price", lit("9.5", "decimal")),
        (ex + "b", ex + "price", lit("0.25", "decimal")),
        (ex + "a", ex + "ok", lit("true", "boolean")),
        (ex + "b", ex + "ok", lit("false", "boolean")),
        (ex + "a", ex + "day", lit("2024-03-05", "date")),
        (ex + "b", ex + "day", lit("2024-12-31", "date")),
        (ex + "a", ex + "at", lit("2024-03-05T10:15:00Z", "dateTime")),
    }


def test_write(tmp_path):
    m = tarka.Mapping.tarql_file(FIXTURES / "extra" / "people.rq")
    df = text_frame("extra/people.csv")
    want = expected("extra/expected/people.nt")
    out = tmp_path / "people.ttl"
    assert m.write(df, out) is None
    assert_same(want, rdflib.Graph().parse(out, format="turtle"), "Turtle file")
    assert_same(want, parse(m.write(df, format="ntriples"), "nt"), "N-Triples text")
    quads = m.write(df, format="nquads", graph="http://example.com/g")
    assert quads and all(line.endswith("<http://example.com/g> .") for line in quads.splitlines())


def test_expand():
    lib = [FIXTURES / "people" / "people.stottr"]
    for name in ("person", "employee"):
        text = tarka.expand(lib, [FIXTURES / "people" / f"{name}.inst.stottr"])
        assert_same(expected(f"people/expected/{name}.nt"), parse(text, "turtle"), f"{name} instances")


def test_shapes():
    m = tarka.Mapping.tarql_file(FIXTURES / "extra" / "people.rq")
    shapes = m.shapes()
    g = parse(shapes, "turtle")
    sh = rdflib.Namespace("http://www.w3.org/ns/shacl#")
    assert (rdflib.URIRef("urn:tarka:shapes:PersonShape"), sh.targetClass, rdflib.URIRef("http://xmlns.com/foaf/0.1/Person")) in g
    assert "@prefix shape: <http://example.com/s#> ." in m.shapes(base="http://example.com/s#")
    with pytest.raises(ValueError):
        m.shapes(base="not an IRI")


def test_validation_with_shacl_engine(tmp_path):
    shacl = pytest.importorskip("shacl")
    m = tarka.Mapping.tarql_file(FIXTURES / "extra" / "people.rq")
    shapes = shacl.Shapes.from_turtle(m.shapes())
    assert shapes.validate_turtle(m.write(text_frame("extra/people.csv"))).conforms
    # an ill-typed value breaks the shapes of a typed OTTR parameter
    (tmp_path / "items.stottr").write_text(
        """@prefix ex: <http://example.com/> . @prefix ottr: <http://ns.ottr.xyz/0.4/> .
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#> . @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        ex:Item [ ottr:IRI ?id, ? xsd:integer ?qty ] :: { ottr:Triple(?id, rdf:type, ex:Item), ottr:Triple(?id, ex:qty, ?qty) } .""",
        encoding="utf-8",
    )
    items = tarka.Mapping.ottr([tmp_path], ["http://example.com/Item"])
    df = pl.DataFrame({"id": ["ex:a", "ex:b"], "qty": ["7", "seven"]})
    report = shacl.Shapes.from_turtle(items.shapes()).validate_turtle(items.write(df))
    assert not report.conforms
    assert [r.focus_node for r in report.results] == ["<http://example.com/b>"]


def test_errors():
    with pytest.raises(ValueError, match="CONSTRUCT"):
        tarka.Mapping.tarql("SELECT * WHERE { }")
    with pytest.raises(ValueError):
        tarka.Mapping.ottr([FIXTURES / "people" / "people.stottr"], ["ex:Nobody"])
    with pytest.raises(OSError):
        tarka.Mapping.tarql_file(FIXTURES / "missing.rq")
    m = tarka.Mapping.tarql_file(FIXTURES / "extra" / "people.rq")
    df = text_frame("extra/people.csv")
    with pytest.raises(ValueError, match="format"):
        m.write(df, format="rdfxml")
    with pytest.raises(ValueError, match="graph"):
        m.write(df, format="nquads")
    # a list parameter's text cells need a separator; a list column needs none
    employee = tarka.Mapping.ottr([FIXTURES / "people" / "people.stottr"], ["ex:Employee"])
    with pytest.raises(ValueError, match=r'lists=\{"skills": ";"\}'):
        employee.triplify(text_frame("people/people.csv"))
