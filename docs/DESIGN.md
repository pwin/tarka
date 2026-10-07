# Design

tarka compiles every mapping, whether a TARQL query or OTTR templates, to one **plan**, and
runs plans with a backend. This document describes the plan, how each front end compiles to it,
how the row backend runs it, how data frames reach it, and how shapes are read from it.

```
 TARQL query ──► tarka-tarql ─┐
                              ├─► Plan ──► row backend (tarka) ──► RDF writer (tarka-io)
 OTTR library ──► tarka-ottr ─┘                   ▲              └► triples frame (tarka-polars)
                                                  ├── CSV records (tarka-io)
                                                  └── frame cells (tarka-polars) ◄── Parquet, IPC, Python

 Plan ──► tarka-shacl ──► SHACL shapes ──► SHACL_Engine ◄── the triples a run makes
```

## 1. The plan

A plan (`tarka_core::Plan`) has two layers.

**Lifting** turns one input record into values for the plan's variables. There are three
kinds:

| Lifting | Used for | How |
|---|---|---|
| `Sparql` | TARQL; OTTR roots with `tq:` annotations | a SPARQL WHERE clause, evaluated with each record's cells bound |
| `Columns` | OTTR roots | each variable reads the column of its name and converts it by type |
| `Given` | OTTR instances | the values are given directly |

A record can give several solutions (a WHERE clause with `VALUES` or `UNION`, for example);
the shape layer runs once per solution.

**Shape** turns a solution into triples. It is a tree of blocks:

* A `Pattern` is a triple of term patterns (constant, variable, blank node, a variable with a
  default, or a list), plus the variables it `requires`. It is emitted when its own terms are
  bound and every required variable is bound. A literal subject or a non-IRI predicate makes no
  triple, as in SPARQL `CONSTRUCT`.
* A `Block` makes fresh blank nodes each time it runs.
* A `Repeat` runs a block once per element of one or more list values: their cross product, or
  zipped to the shortest or the longest list.

A list value in object position becomes an RDF collection.

## 2. TARQL

The CONSTRUCT template becomes the root block: one pattern per template triple, and one blank
node per template blank node, fresh for every solution. The WHERE clause becomes a `Sparql`
lifting.

**Binding rows.** TARQL binds each row as a solution that the WHERE group starts from, as if
`VALUES (?col …) { (…) }` came first in the group. spargebra has already turned the group into
algebra, in which BINDs and FILTERs wrap the patterns they follow and joins fold to the left. So
`tarka-tarql` puts a VALUES table of the batch's rows in place of the group's left-most pattern,
below the projection spargebra adds and below any solution modifiers. A hidden variable carries
each row's position in the batch, so solutions go back to their rows.

* Only columns the query mentions are bound. A column with the same name as a variable the
  group itself BINDs is left out, so the BIND wins.
* `?ROWNUM` is bound to the row number (from 0), unless the CSV has a column of that name.
* With solution modifiers at the top (`LIMIT`, `ORDER BY` …), rows are evaluated one at a time,
  so the modifiers apply per row, as in oxi-gen.
* spareval's `BNODE()` makes random labels; they are renamed per record so output is stable.

Because the row's cells are bound, not substituted, `BOUND(?column)` is true when the cell has a
value. This is TARQL's behaviour; oxi-gen differs (see the README).

**Checking the query.** Before reading input, the WHERE clause is evaluated once on a record of
empty cells. A function spareval does not have (`xsd:date()`, for example) fails there, with a
message, instead of failing on the first row.

**Prefixes.** spargebra does not keep the prologue, which `tarql:expandPrefixedName` needs, so
`tarka-tarql` scans it separately (skipping comments, strings and IRIs).

## 3. OTTR

A template compiles by symbolic expansion down to `ottr:Triple`:

| OTTR | In the plan |
|---|---|
| a mandatory parameter given a variable | the variable is required by every pattern the instance makes, so the whole instance disappears when it is unbound |
| a mandatory parameter given `none` | the instance is dropped when compiling |
| a default, given a variable | `Default(variable, fallback)` |
| a blank node in a template body | a blank node of the block the instance is expanded into: fresh per instance |
| `cross`/`zipMin`/`zipMax` over constant lists | unrolled when compiling |
| `cross`/`zipMin`/`zipMax` over a list value | a `Repeat`, with one element variable per list |
| a list term | an RDF collection |

The root template's parameters become the plan's variables. Where their values come from:

1. **`tq:` annotations** on the root (`tq:Bind`, `tq:Where`, `tq:Prefix` …, as ottr2sparql's
   `decompose` writes them): a `Sparql` lifting built from them. Parameters that no BIND makes
   are read from columns of the same name.
2. Otherwise **columns**: each parameter reads the column of its name, converted by its type.
   IRI types (`ottr:IRI` and its OWL subtypes) take a full IRI (containing `://`) or a prefixed
   name; `xsd:*` types (except `xsd:string`) make a typed literal; anything else a plain
   string. A list type splits the cell on its separator and converts each part. A parameter
   named `ROWNUM` takes the row number.
3. For **instances**, the given arguments.

**Several roots.** `compile_many` instantiates several root templates for each record. Each
root's parameters read the columns of the same names; roots share a variable when they convert a
column the same way, and read it separately when their types differ. A root's mandatory
parameters drop only its own instance.

**Libraries.** A library is every `.stottr` file under the paths given. A template with a
pattern wins over a signature of the same IRI, whatever the order. Only `/*** … ***/` block
comments are allowed, as in Lutra.

## 4. Typed values

spareval writes every typed value an expression makes in canonical form (`"129.90"` as
`"129.9"^^xsd:decimal`, `"1"` as `true`, `"…+00:00"` as `"…Z"`). The OTTR route's conversions use
the same forms (through `oxsdatatypes`, which spareval uses), so a TARQL mapping and its OTTR
equivalent give the same terms. A value outside a datatype's lexical space is kept as written,
an ill-typed literal, as `STRDT` keeps it.

One difference is deliberate. spareval changes the datatype of `STRDT(?x, xsd:int)` and the other
types derived from `xsd:integer` to `xsd:integer`, and of `xsd:dateTimeStamp` to `xsd:dateTime`.
The OTTR route keeps the declared datatype.

## 5. The row backend

`tarka::run` reads records in batches (512 by default), lifts and shapes several batches in
parallel, and writes each record's triples in input order. Blank node labels are made from a
record's position in the input (`b{record}x{n}`), so the output does not depend on how the work
was split. Duplicates are removed within each record, or within a window of distinct triples
(`--dedup N`), as in oxi-gen; Turtle output sorts each window, so a subject's triples come out
together.

## 6. Data frames

A record's cells are text or lists (`Cell::Text`, `Cell::List`). tarka-polars converts a frame
column by column, and only the columns the plan reads: each Polars type has one text form (see
the README), and a `List` column gives list cells. The row backend then runs unchanged, so a
frame gives the same RDF as a CSV file holding the same text, and the tests check exactly that
on every fixture.

The text forms are chosen so that conversion by an OTTR type gives the canonical literal: a
`Float64` is written in its shortest round-trip form, a `Datetime` with a time zone in UTC
with `Z`. A list cell feeds an OTTR list parameter directly. A column binding records whether
its parameter takes a list, and the check that cells fit happens as they are read, since only
the input says whether a cell is text or a list: a text cell for a list parameter needs a
separator, and a list cell for any other parameter is an error. So is a list column used by a
TARQL query, whose rows bind strings.

Converting to cells costs a copy of the used columns. Evaluating `Columns` liftings as Polars
expressions, without the copy, is possible later; the plan does not need to change for it.

## 7. Python

`tarka-python` is a PyO3 module built with maturin. A `Mapping` holds a compiled plan; frames
cross with pyo3-polars, which passes each column's chunks through the Arrow C data interface
without copying. Mapping runs release the GIL. Errors become `ValueError`, or `OSError` when a
file cannot be read.

## 8. Shapes

`tarka-shacl` reads a plan's shape layer as a description of the RDF it makes.

**Where a subject is known to exist.** Each triple pattern sits in a block (the root, or a
repeat's body) and needs some variables bound: those in its terms (a default needs only its
fallback's), those it `requires`, and those of the repeats around it. A pattern P is made
whenever pattern T is made when P's block encloses T's and P needs nothing T does not, besides
variables bound in every solution (the row number). A class assertion (`?s rdf:type C`, C
constant) says where its subject exists; a shape for C collects the patterns on that subject,
and a property is required when it is made whenever the class assertion is, in every place C is
asserted.

**What values are.** A variable's kind comes from its lifting. Columns lifting: the OTTR
conversion (an IRI, a typed or plain literal, a list), with list elements bound by repeats
taking the element conversion. SPARQL lifting: the expression its BINDs give it, inferred over
SPARQL's functions, XSD casts and TARQL's, through `IF` and `COALESCE`; a variable no BIND sets
can only hold a column's string. Casts follow what spareval writes: `xsd:int` values come out as
`xsd:integer`. A property's kinds are those of all its objects, and become `sh:datatype` when
they are one datatype, else `sh:nodeKind` when they agree on one.

**Objects.** An object gets `sh:class C` when its own class assertion is made whenever the
property is; a blank node object without a class gets a `sh:node` shape built the same way.

The tests generate shapes for every fixture mapping and validate the mapping's output against
them, which checks the inference against what the engine really makes.

**Validation** runs SHACL_Engine in process: tarka's triples and the shapes go into its term
store directly. SHACL_Engine builds the Oxigraph crates with their RDF 1.2 features, and Cargo
turns features on for the whole build, so spargebra then parses SPARQL 1.2. tarka's RDF 1.1
scope is therefore checked explicitly (`tarka_tarql::sparql11`): triple terms, SPARQL 1.2
functions and literals with a base direction are refused whichever way spargebra was built, and
the tests run both ways. The Python module leaves validation to SHACL_Engine's own package and
is built without it.

## 9. What is next

* Loading into HOLOS (and other stores) over the SPARQL Graph Store Protocol.
* Mapping files (in the spirit of bOTTR's argument maps: language tags, null values, IRI
  templates), and a linter for template libraries.
