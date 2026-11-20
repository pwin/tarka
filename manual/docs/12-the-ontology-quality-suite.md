# 12. The Ontology Quality Suite

SHACL checks what you wrote rules for, and SPARQL what you thought to ask. The
[Ontology Quality Suite](https://pypi.org/project/ontology-quality-suite/) brings a registry of
checks someone else thought of: sixty-odd of them, over the ontology, the mappings and the data,
each with an identifier, a severity and a remedy. It is the gate of the
[SemOps Manual](https://www.semantechs.co.uk/SemOps_Manual/)'s toolchain, and
[its Chapter 10](https://www.semantechs.co.uk/SemOps_Manual/10-ingest-and-transform.html) runs
it around the ingest stage. This chapter runs two of its stages on the trail: `sketch`, on the
mappings, before any data is read, and `data`, on what tarka made.

```sh
pip install ontology-quality-suite        # the command is ontology-quality-suite
```

## Before any data: sketch the mappings

`sketch` reads TARQL queries as text, works out the graph they would make, and holds it up
against the ontology: does the mapping use terms the ontology declares, in the way it declares
them?

```sh
ontology-quality-suite sketch --queries tarql/ \
    --query-pattern "bookshops.rq,stocks.rq,stock.rq,events.rq,works.rq" \
    --ontology reference/01-vocabulary.ttl --out-dir out/oqs/sketch --fail-on never
```

The first time this manual's queries were sketched, the suite reported:

```text
Findings: 29 total (4 Violation, 2 Warning, 23 Info)
Sketch used 5 query file(s); 48 triples, 34 entities.
```

`findings.txt` in the output folder says what each is, where, and how to fix it. Three of the
six serious ones were fair, and changed the queries:

- **TQL-001, Warning.** *?price is bound in 3 query files by 2 structurally different
  expressions.* It was: a shelf price in `stock.rq`, a recommended price in `works.rq`, a ticket
  price in `events.rq`, all called `?price`. Nothing was wrong in any one query, but a reader
  moving between them would assume one meaning. They are `?shelf_price_value`, `?rrp_value` and
  `?ticket_price_value` now.
- **CNF-001, Warning, and two CNF-003, Violation.** `works.rq` made a translation's class a
  variable, `?work a bs:Work, ?translation_class`, bound to `bs:Translation` only for a
  translation. The output was right, but the sketch could not see through the variable: it took
  `?translation_class` for an undeclared class, and so found `bs:translationOf` used on something
  that was not a `bs:Translation`. The query now makes the *subject* conditional instead, and
  keeps the class constant, which any reader can see:

  ```sparql
  ?translation a bs:Translation ;
      bs:translationOf ?original_iri ;
      bs:translatedBy  ?translator_iri .
  …
  BIND(IF(BOUND(?translation_of), ?work, ?unset) AS ?translation)
  ```

  The output is the same graph, triple for triple ([Chapter 11](11-sparql-checks.md)'s round trip
  says so). A mapping that a static check can read is easier for people to read, too.

The sketch again:

```text
Findings: 24 total (2 Violation, 0 Warning, 22 Info)
```

What remains:

- **CNF-005, Info (20).** Classes the ontology declares that the sketch never populates: places,
  people, publishers. Expected: they are reference data, and no spreadsheet makes them.
- **TQL-003, Info (3).** `?isbn`, `?kind` and `?language` are used in a template and bound
  nowhere in the query. In TARQL that is ordinary, since each is a column, and the suite says so,
  asking only that someone check the columns exist.
- **CNF-003, Violation (2).** *shop_iri uses property bs:locatedIn but its type doesn't match any
  of the property's declared domain classes*, and the same for `bs:founded`. These are not
  violations. The domain of both is a union, `[ owl:unionOf ( bs:Bookshop bs:Publisher ) ]`,
  and a bookshop is in it; the suite, at 0.27.0, does not read a union in `rdfs:domain`. Know
  your tools' edges: a finding is a question, and this one has an answer.

## After the run: assess the data

`data` assesses RDF itself: structure, conformance to the ontology, logic (with OWL RL
reasoning), data quality. It wants what tarka made and the reference data it points into, so
that the things the output links to have their types:

```sh
ontology-quality-suite data out/bookshops.ttl out/stocks.ttl out/stock.ttl out/events.ttl \
    out/works.ttl out/segments.ttl out/connections.ttl \
    reference/02-genres.ttl reference/03-places.ttl reference/05-people.ttl \
    --ontology reference/01-vocabulary.ttl --own-namespace https://example.org/bookshop-trail/ \
    --engine sparql --out-dir out/oqs/data --fail-on never
```

```text
Findings: 634 total (181 Violation, 447 Warning, 6 Info)
```

Six hundred and thirty-four is a number to sort, not to fear. By check:

| Findings | Check | Severity | What it is about |
|---:|---|---|---|
| 228 | QUA-004 | Warning | resources without `skos:prefLabel` |
| 83 | QUA-009 | Warning | classes and properties without one `skos:prefLabel` per language |
| 83 | QUA-010 | Warning | classes and properties without `skos:definition` |
| 92 | CNF-003 | Violation | `bs:founded` and `bs:locatedIn` on shops and publishers: the union domain again |
| 78 | LOG-002 | Violation | `bs:locatedIn` and `bs:basedIn`, functional, with "more than one distinct value" |
| 41 | DAT-002 | Warning | references to things not described: 30 DBpedia places, 11 series |
| 11 | STR-006 | Violation | objects with no type: the same 11 series |
| 11 | STR-004 | Warning | classes with no formal definition |
| 7 | (others) | | one property without domain or range, a duplicate literal, no `owl:versionIRI` … |

Read it in three piles.

**About the ontology, not the transformation.** QUA-004, -009, -010 and STR-004, 405 findings,
say the vocabulary labels its terms with `rdfs:label` rather than SKOS, and defines them in
comments. That is a choice the Ontology course makes and discusses; it is the modeller's
conversation, and no mapping can settle it. Scope a data assessment to what the data team owns.

**False positives.** The 92 CNF-003s are the union domain the sketch also met. The 78 LOG-002s
are subtler. The places are `owl:sameAs` their DBpedia resources, so after OWL RL reasoning a
shop is `bs:locatedIn` both `bt:place-wigtown` and `dbr:Wigtown`. They are one place under two
names, and a functional property may have one value under many names; the suite, at 0.27.0,
counts the names. Both are the suite's to fix, and neither is about tarka.

**Real findings.** Eleven works are `bs:inSeries` a series that nothing in the data describes
(STR-006 and DAT-002): the committee keeps no spreadsheet of series, so the links point into
nothing. tarka mapped exactly what it was given, and the assessment shows what was never given.
That is the finding a data team needs: not a broken mapping, but a missing source. (The thirty
DBpedia references are external links, as intended.)

The report is `report.html`, with every finding located in the source where it can be, and
`findings.txt`, plain text with the source line beside each.

## Where the suite fits

- **Sketch in review.** It reads queries, not data, so it runs on every pull request that
  changes a mapping, in seconds.
- **Assess after each run.** Scope it with `--own-namespace`, and track the counts between runs:
  a change in them is the signal. [SemOps Manual §10.4](https://www.semantechs.co.uk/SemOps_Manual/10-ingest-and-transform.html)
  shows how the counts move between suite versions, and why.
- **Gate on what you own.** `--fail-on Violation` stops a pipeline. Turn it on once the false
  positives above are understood, or scoped out; [Chapter 14](14-a-pipeline.md) shows the gate in
  a pipeline.

The suite also has `triplify`, which drives oxi-gen; for tarka, run tarka and give `data` its
output, as here. [SemOps Manual, Chapter 9](https://www.semantechs.co.uk/SemOps_Manual/09-continuous-integration.html)
covers the suite in continuous integration, and its
[Chapter 14](https://www.semantechs.co.uk/SemOps_Manual/14-coverage-and-gaps.html) what it
covers and what it does not.

---

| [← 11. SPARQL checks](11-sparql-checks.md) | [13. Into a store →](13-into-a-store.md) |
|---|---|
