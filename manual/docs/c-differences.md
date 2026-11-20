# Appendix C. Differences

tarka gives the RDF of three tools. Where they disagree with each other, or tarka with one of
them, it is deliberate, and here.

## From TARQL (TARQL 1.3, on Jena 4.8)

| | TARQL | tarka |
|---|---|---|
| `?ROWNUM` | counts from 1 | counts from 0, as oxi-gen does |
| A backslash in a quoted CSV field | taken literally | escapes the next character, as in oxi-gen; change it with `-p` |
| File encoding | the platform's (on Windows, not UTF-8) | UTF-8, everywhere |
| Typed values | as Jena writes them: `"2.0e0"^^xsd:double` | canonical: `"2"^^xsd:double` (the same value) |
| `afn:sprintf` | formats as Java's `String.format` | refused, with a message |
| `afn:bnode` | a blank node's label | not available (tarka's labels are made afresh) |
| Property functions that read the data (`list:member`, `rdfs:member`, `apf:bag` …) | match nothing in a row | refused, with a message |
| A property function inside `OPTIONAL`, `UNION`, a nested group | evaluated | refused, with a message |

Jena's other property functions and functions give what TARQL 1.3 gives, which
`tests/fixtures/jena` records and the tests check.

## From oxi-gen

| | oxi-gen | tarka |
|---|---|---|
| `BOUND(?column)` | always false: cells are substituted into the query, so `FILTER(BOUND(?column))` aborts it | true when the cell has a value, as in TARQL |
| A function the engine does not have | a crash | a message, before any row is read |
| A malformed row | a crash | a message naming the row; the file is not written |
| `-o FILE` when a run fails | a partial file | the earlier file, kept |
| Blank node labels | random per run | stable: the same input gives the same output |
| Jena's `apf:` and `afn:` functions | none | as TARQL has them |

## From Lutra

tarka's OTTR, bOTTR and `expand` output is Lutra's on every fixture in the repository, and the
linter reports what Lutra's does. The differences are in what is reported, not what is made:

| | Lutra | tarka |
|---|---|---|
| A literal of a datatype OTTR does not have, read with `ottr:datatypeSep` | written, with a warning that the datatype is unusual | written |
| `ottr:type` naming a type OTTR does not have | an error per row; the values written as plain strings | a warning, once; the values written as plain strings |
| A relative path in an H2 query | from the working directory | from the bOTTR file's directory (`@@THIS_DIR@@` works in both) |
| SQL beyond `SELECT … FROM CSVREAD(…)` | run by H2 | refused (tarka does not embed H2) |
| JDBC sources | read | not read |
| `tarka run -l` (OTTR over a table, by column name) | no such feature | tarka's own |

## From the RDF it reads and writes

tarka works with RDF 1.1 and SPARQL 1.1. Its engine (HOLOS's) parses RDF 1.2 and SPARQL 1.2, so
tarka refuses them explicitly: a triple term, a SPARQL 1.2 function, a literal with a base
direction, in a query, a template or an endpoint's answer. A source that writes a base
direction, such as the Bookshop Trail's three right-to-left titles (`"…"@ar--rtl`), can be mapped
with the language and not the direction.

---

| [← Appendix B. Python reference](b-python-reference.md) | [Appendix D. Troubleshooting →](d-troubleshooting.md) |
|---|---|
