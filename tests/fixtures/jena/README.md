# Jena's functions in TARQL queries

Queries that use Jena ARQ's property functions (`apf:strSplit`, `apf:splitIRI`, `apf:concat`,
`apf:str`, `apf:assign`) and functions (`afn:localname`, `afn:strjoin` …), as queries written
for TARQL proper do, and the CSV files they read.

`expected/` holds what TARQL 1.3 (Jena 4.8.0) made of each, its triple lines sorted:

```sh
java -jar tarql-1.3-all.jar --ntriples strsplit.rq strsplit.csv | grep -E '^(<|_:)' | sort > expected/strsplit.nt
```

(`strsplit-regex.rq` reads `strsplit.csv` too.) `crates/tarka/tests/jena.rs` requires tarka to
make the same RDF, comparing typed literals by value: tarka writes them in canonical form,
where Jena writes `"2.0e0"^^xsd:double`, tarka writes `"2"^^xsd:double`. With `TARQL_JAR` set to
TARQL's jar, the test runs TARQL again.
