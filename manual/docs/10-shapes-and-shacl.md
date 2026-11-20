# 10. Shapes and SHACL

A mapping that runs is not yet a mapping that is right. This part of the manual is about
knowing: this chapter with SHACL, the next with SPARQL, and the one after with the Ontology
Quality Suite. Each finds things the others cannot, and the faulty spreadsheet,
`data/faulty/bookshops.csv`, gives them five mistakes to find:

| Row | Cell | Mistake |
|---|---|---|
| Marginalia | `town` | `wigton`: Wigtown misspelt |
| Borderprint | `founded` | `c.1972`: "about 1972" is not a year |
| The Bookwyrm | `staff` | `0`: nobody works there? |
| Bookbarrow | `website` | `www.example.org/shops/bookbarrow`: no `https://` |
| Castle Steps Books | `second_hand` | `Yes`: the column says `yes` |

Map it, and every row maps. `tarka run` exits with status 0, and writes 33 shops; nothing so far
has looked at what they say.

## Shapes from the mapping

tarka can say what a mapping makes, as SHACL shapes:

```sh
tarka shapes -q tarql/bookshops.rq --base https://example.org/bookshop-trail/shapes# -o out/bookshops.shapes.ttl
```

```turtle
shape:BookshopShape a sh:NodeShape ;
    sh:targetClass bs:Bookshop ;
    sh:property
        [ sh:path geo:hasDefaultGeometry ; sh:nodeKind sh:IRI ; sh:class geo:Geometry ; sh:class sf:Point ] ,
        [ sh:path rdfs:label ; sh:datatype <http://www.w3.org/1999/02/22-rdf-syntax-ns#langString> ] ,
        [ sh:path wgs84:lat ; sh:datatype xsd:decimal ] ,
        [ sh:path bs:floorArea ; sh:datatype xsd:decimal ] ,
        [ sh:path bs:founded ; sh:datatype xsd:gYear ] ,
        [ sh:path bs:hasCafe ; sh:nodeKind sh:Literal ] ,
        [ sh:path bs:locatedIn ; sh:nodeKind sh:IRI ] ,
        [ sh:path bs:staffCount ; sh:datatype xsd:integer ] ,
        [ sh:path bs:website ; sh:datatype xsd:anyURI ] ,
        …
```

Each class the mapping asserts gets a shape, and each property what the mapping binds: a
datatype from a cast or `STRDT`, `sh:nodeKind sh:IRI` from `IRI()`, `sh:class` where the object
is always typed. The shapes are honest about TARQL: no property is required (`sh:minCount`),
because any cell may be empty, and `bs:hasCafe` is only a literal, because the `IF` may give
nothing. From OTTR templates the shapes say more, since a template's signature declares which
parameters are mandatory: those properties get `sh:minCount 1`.

These shapes describe the mapping, not the trail. They catch output that does not match its own
mapping (a value of the wrong datatype, a literal where an IRI belongs) and are a starting
point for your own.

## Validating as you write

`--validate` checks the output against the mapping's shapes, `--shapes FILE` against your own
(repeatable), and `--report FILE` writes the SHACL report. `shapes/trail-extra.ttl` holds the
manual's own rules, beyond the course's: events and stock records, and that a shop answers the
spreadsheet's yes/no questions:

```turtle
bt:BookshopExtraShape a sh:NodeShape ;
    sh:targetClass bs:Bookshop ;
    sh:property [ sh:path bs:sellsSecondHand ; sh:minCount 1 ;
                  sh:message "No yes or no for second-hand books: is the cell 'Yes', or empty?" ] ,
        [ sh:path bs:hasCafe ; sh:minCount 1 ;
          sh:message "No yes or no for a cafe: is the cell 'Yes', or empty?" ] ,
        [ sh:path bs:founded ; sh:pattern "^[0-9]{4}$" ;
          sh:message "A founding year is four digits: 'about 1972' belongs in a note, not in the year." ] .
```

The good spreadsheet passes:

```sh
tarka run -q tarql/bookshops.rq -i data/bookshops.csv --validate --shapes shapes/trail-extra.ttl -o out/bookshops-validated.ttl
```

The faulty one does not:

```sh
tarka run -q tarql/bookshops.rq -i data/faulty/bookshops.csv --validate --shapes shapes/trail-extra.ttl \
    --report out/faulty-report.ttl -o out/faulty.ttl
```

```text
  bt:shop-borderprint bs:founded sh:DatatypeConstraintComponent: "c.1972"^^xsd:gYear
  bt:shop-borderprint bs:founded sh:DatatypeConstraintComponent: "c.1972"^^xsd:gYear
  bt:shop-borderprint bs:founded sh:PatternConstraintComponent: "c.1972"^^xsd:gYear
  bt:shop-castle-steps bs:sellsSecondHand sh:MinCountConstraintComponent
tarka: the output does not conform to the shapes (4 results)
```

tarka exits with status **3**, and the output is written all the same: a file that does not
conform is still the file the mapping makes, and a pipeline decides what to do with it. The
report, in Turtle, carries the messages:

```turtle
_:r1 a sh:ValidationResult ;
	sh:focusNode bt:shop-borderprint ;
	sh:resultMessage "A founding year is four digits: 'about 1972' belongs in a note, not in the year." ;
	sh:resultPath bs:founded ;
	sh:resultSeverity sh:Violation ;
	sh:sourceConstraintComponent sh:PatternConstraintComponent ;
	sh:value "c.1972"^^xsd:gYear .
```

Two of the five, found as the file was written. The year breaks the mapping's own shapes as
well as the pattern: twice, because the shop is both a `geo:Feature` and a `bs:Bookshop`, and
the mapping's shapes give each class its own rule for `bs:founded`. `Yes` made the mapping's
`IF` give nothing, so the shop has no `bs:sellsSecondHand` at all, which only a shape that
requires one can notice.

### Why a pattern for the year?

`STRDT("c.1972", xsd:gYear)` writes `"c.1972"^^xsd:gYear`: an *ill-typed* literal, its text not
in the datatype's lexical space. `sh:datatype xsd:gYear` rejects it, as above, but only in a
validator that checks a gYear's lexical form. SHACL_Engine checks it from 0.3.9 (in tarka
0.3.1) and HOLOS from 0.27.0. Their earlier releases rejected `"seven"^^xsd:integer`,
`"2025-02-31"^^xsd:date` and `"yes"^^xsd:boolean`, and let `"c.1972"^^xsd:gYear` through, as
other validators may still. A pattern states the rule outright, whichever engine reads it, and
its message says what a year should be. Where a value's form matters, say what the form is.

## Validating with the trail's own shapes

The SHACL course ends with one shapes graph for the whole trail,
`reference/s64-the-whole-trail-in-one-shapes-graph.ttl`: the rules the trail's data must keep,
written by the people who know the trail. They are the real test. Most are about one node, but
some reach further: a shop's `bs:locatedIn` must be a `bs:Settlement`, and following
`bs:within` from that settlement must reach exactly one country. tarka's output does not contain
the settlements. They are reference data. So s64 must run over the output **and** the
reference data together, which is a job for a store: `python/validate.py` loads both into a
HOLOS store and validates there.

```python
store = holosdb.Store()
for name in ["01-vocabulary.ttl", "02-genres.ttl", "03-places.ttl", "05-people.ttl"]:
    store.load(f"reference/{name}")
for f in files:
    store.load(f)
report = store.validate("reference/s64-the-whole-trail-in-one-shapes-graph.ttl")
```

Over everything Part II and Part III made, the good spreadsheets give:

```sh
python python/validate.py out/bookshops.ttl out/stocks.ttl out/stock.ttl out/events.ttl \
    out/works.ttl out/segments.ttl out/connections.ttl
```

```text
bookshops.ttl against the mapping's shapes: conforms
everything against s64, with the reference data: 0 violations, 6 warnings
   Warning: …author-kirsty-lammond  …SPARQLConstraintComponent: …author-kirsty-lammond is in a cycle of influence. Three authors are, by design (q34).
   …
   Warning: …event-severn-leaf-2025-04-05  …SPARQLConstraintComponent: …event-severn-leaf-2025-04-05 features …author-perrin-oakes, who had died by then. A memorial, or a mistake?
   Warning: …shop-penwith  …SPARQLConstraintComponent: …shop-penwith cannot be reached on foot from The Inkwell. Two shops cannot, by design.
```

No violations. The six warnings are the course's own puzzles, seeded in its data on purpose
(three authors who influence each other in a circle, an event featuring an author who had died,
two shops beyond the footpaths), and tarka's output reproduces them faithfully, because it
reproduces the data. SHACL calls a report with any result, even a warning, non-conforming; what
a pipeline should stop on is a violation.

Now the faulty bookshops, with the same other files:

```sh
python python/validate.py out/faulty.ttl out/stocks.ttl out/stock.ttl out/events.ttl \
    out/works.ttl out/segments.ttl out/connections.ttl
```

```text
faulty.ttl against the mapping's shapes: does not conform
   …shop-borderprint …schema#founded DatatypeConstraintComponent
   …shop-borderprint …schema#founded DatatypeConstraintComponent
everything against s64, with the reference data: 5 violations, 6 warnings
   Violation: …shop-bookbarrow …schema#website …PatternConstraintComponent: At most one website, with a scheme: {$value}.
   Violation: …shop-bookwyrm …schema#staffCount …MinInclusiveConstraintComponent: Staff count is a positive integer: {$value}.
   Violation: …shop-borderprint …schema#founded …DatatypeConstraintComponent: A shop records the one year it opened, as an xsd:gYear.
   Violation: …shop-marginalia …schema#locatedIn …ClassConstraintComponent: A shop is in exactly one settlement.
   Violation: …shop-marginalia _:… …QualifiedMinCountConstraintComponent: {$this} is in some number of countries other than one.
```

Four of the five mistakes, in five violations, found by the course's rules: the website without
a scheme, the shop with no staff, the year, and `wigton`, which breaks two rules, since it is not
a settlement and so the shop is in no country at all. Only validation with the reference data
could find `wigton`: `bt:place-wigton` is a perfectly good IRI, and the shapes from the mapping,
which know nothing of places, are content. Only `Yes` gets past s64, which lets a shop say
nothing about second-hand books.

## Who found what

| Mistake | Mapping's shapes | `trail-extra.ttl` (`tarka --shapes`) | s64 with reference data |
|---|---|---|---|
| `wigton` |  |  | yes: not a settlement, no country |
| `c.1972` | yes: datatype | yes, by pattern | yes: datatype |
| staff `0` |  |  | yes: below 1 |
| no `https://` |  |  | yes: pattern |
| `Yes` |  | yes: required |  |

No one set of shapes found everything. The mapping's shapes guard the mapping; the project's
own shapes guard what the spreadsheets must answer; the domain's shapes, run with the data they
refer to, guard the meaning. Use all three: the first two as the file is written
(`--validate --shapes`), the third where the reference data is, in a store.

## SHACL from Python, and from the command line

From Python, without a store, SHACL_Engine validates text:

```python
import shacl, tarka_rdf
m = tarka_rdf.Mapping.tarql_file("tarql/bookshops.rq")
report = shacl.Shapes.from_turtle(m.shapes()).validate_turtle(m.write(df))
report.conforms, [(r.focus_node, r.path, r.component) for r in report.results]
```

and `shacl.validate("data.ttl", "shapes.ttl")` validates files. SHACL_Engine's own command
line, from its [releases](https://github.com/pwin/SHACL_Engine/releases), does the same in a
shell. HOLOS's `holos validate` validates a store.

The [SHACL course](https://www.semantechs.co.uk/SHACL_Course/) teaches every shape in s64, and
[SemOps Manual, Chapter 8](https://www.semantechs.co.uk/SemOps_Manual/08-model-and-validate.html)
puts validation into the modelling stage of a semantic practice.

---

| [← 9. Python and Polars](09-python-and-polars.md) | [11. SPARQL checks →](11-sparql-checks.md) |
|---|---|
