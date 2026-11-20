# Appendix D. Troubleshooting

The messages you are most likely to meet, what they mean, and what to do. Most of them came up
while this manual was written.

## The run made nothing, and said nothing

```text
2 records, 2 solutions, 0 triples made, 0 written
```

`--stats` shows it. With OTTR templates over a table (`tarka run -l`), each parameter reads the
column **of the same name**; a parameter with no column is `none` in every row, and a mandatory
one then drops every instance. Check the column names against the root template's parameters
(`floor_area` is not `?floorArea`), or use a bOTTR map, whose columns are positional.

With a TARQL query, a template variable that is never bound (a misspelt column name) leaves its
triples out on every row. The Ontology Quality Suite's `sketch` reports such variables (TQL-003).

## -o didn't write the file

```text
tarka: cannot write out.ttl: Access is denied. (os error 5)
```

The file is open in a program that locks it (a spreadsheet, some editors). Close it and run again;
the earlier file is kept until then. `-o` writes all or nothing: a run that fails for any reason
leaves no partial file, so a missing file after an error is by design. A relative `-o` is from the
directory you run the command in.

## The file is Turtle, though it is called .nt

The format does not follow the file name. Add `--ntriples` (or `--format ntriples`).

## N-Quads output needs --graph

N-Quads names a graph for every triple: give it with `--graph IRI`.

## left out: row 3: column shop: 'Border Print' is not an IRI

A cell cannot become the term its parameter needs. The row is left out, the others are made, and
tarka exits with status 1. Fix the cell, or, in Python, pass `strict=False` to keep going without
an exception.

## A cell became a strange IRI: `<geom:shop-bookbarrow>`

The cell used a prefix the library (or bOTTR file) does not declare, so it was read as a full IRI
with that scheme. Declare every prefix the cells use.

## parameter ?records has a list type: give the separator of its text cells

A text column feeding a list parameter needs `--list records ';'` (or `lists={"records": ";"}`
in Python). A Parquet or Arrow list column needs nothing.

## the value 'Yes' is not in the lexical space of its datatype xsd:boolean

A bOTTR argument map read a cell its type cannot hold. Fix the cell, or teach the map the word
(`ottr:booleanTrue ( "yes" "Yes" )`).

## expected a column, not from

A bOTTR H2 query names a column after an SQL keyword (`from`, `to`, `order` …). Rename the column.

## ottr:type <…#wktLiteral> is not a type OTTR has

OTTR has no type for that datatype, so the values are plain strings. Write the datatype in the
cell (`…^^geo:wktLiteral`) and read it with `ottr:datatypeSep "^^"`.

## the output does not conform to the shapes (2 results)

The output was written, and breaks the shapes; tarka exits with status 3. The lines above the
message, and `--report FILE`, say which nodes and why.

## An ill-typed value got past sh:datatype

`STRDT("c.1972", xsd:gYear)` writes `"c.1972"^^xsd:gYear`, and not every validator checks every
datatype's lexical form: SHACL_Engine 0.3.8 and HOLOS 0.26.4 do not check `xsd:gYear`'s. Add a
`sh:pattern` where the form matters, or cast where SPARQL has a cast for the type (a failed cast
binds nothing, and a required-property shape then reports it).

## Validation with my own shapes reports a class constraint I am sure holds

`sh:class` is checked against the data graph: if the object's type is in reference data that is
not loaded (places, genres, people), the constraint fails. Validate tarka's output together with
the reference data, in a store ([Chapter 10](10-shapes-and-shacl.md)).

## X is not supported

A query uses something tarka will not evaluate as written: a property function that reads the
data, `afn:sprintf`, a property function inside `OPTIONAL`, an RDF 1.2 feature, a function the
engine does not have. [Chapter 5](05-tarql-in-depth.md#what-tarka-refuses) lists them.

## The library has 5 errors

`tarka lint` found errors in a template library; each line names the file, the line, the rule and
usually the fix. [Chapter 6](06-templates.md#checking-a-library-tarka-lint) works through them.

## Output differs from run to run

It should not: tarka's output is the same for the same input, blank nodes from `BNODE()` included
(tarka names them per row). If it differs, check for `afn:now()`, `NOW()`, `UUID()`, `STRUUID()`
or `RAND()` in the query, which change by design; and compare graphs by value, not lexical form, when two *different* routes are involved
(bOTTR writes `"80.0"` where a TARQL cast writes `"80"`).

---

| [← Appendix C. Differences](c-differences.md) | [Appendix E. The example files →](e-the-example-files.md) |
|---|---|
