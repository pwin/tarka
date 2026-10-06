# Design

tarka compiles every mapping, whether a TARQL query or OTTR templates, to one **plan**, and
runs plans with a backend. This document describes the plan, how each front end compiles to it,
and how the row backend runs it.

```
 TARQL query ──► tarka-tarql ─┐
                              ├─► Plan ──► row backend (tarka) ──► RDF writer (tarka-io)
 OTTR library ──► tarka-ottr ─┘                   ▲
                                                  └── CSV records (tarka-io)
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

## 6. What is next

* A column-wise backend over Polars data frames, with Parquet input and Python bindings.
* SHACL shapes generated from plans, validation with SHACL_Engine, and loading into HOLOS.
* Mapping files (in the spirit of bOTTR's argument maps: language tags, null values, IRI
  templates), and a linter for template libraries.
