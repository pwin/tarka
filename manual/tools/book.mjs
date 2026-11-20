/**
 * The tarka Manual's chapters and details, shared by every build: the diagrams, the web
 * edition, the EPUB and the A4 PDF.
 */
import path from 'path';

export const MANUAL = path.resolve(path.dirname(new URL(import.meta.url).pathname).replace(/^\/([A-Za-z]:)/, '$1'), '..');
export const BUILD = path.join(MANUAL, 'build');
export const DIAGRAMS = path.join(BUILD, 'diagrams');

export const TITLE = 'The tarka Manual';
export const SUBTITLE = 'From tables to triples along the Bookshop Trail';
export const AUTHOR = 'Peter Winstanley';
export const DESCRIPTION =
  'The tarka Manual: TARQL queries, OTTR templates and data frames to RDF, on the Bookshop Trail, and how to know the RDF is right with SHACL, SPARQL and the Ontology Quality Suite.';

/** Where the manual's own files live on GitHub, for links into the repository. */
export const GITHUB = 'https://github.com/pwin/tarka';
export const GITHUB_TREE = `${GITHUB}/tree/main/manual`;
export const GITHUB_BLOB = `${GITHUB}/blob/main/manual`;
/** The EPUB and the PDF are release assets, not committed files. */
export const RELEASES = `${GITHUB}/releases/latest`;
export const SITE_HOME = '/';

/** Reading order, grouped by part. `page` is the web edition's file name. */
export const PARTS = [
  { title: null, chapters: [{ file: 'README.md', page: 'index.html', label: '', short: 'Introduction' }] },
  { title: 'Part I — Setting out', chapters: [
    { file: 'docs/01-what-tarka-does.md', label: '1' },
    { file: 'docs/02-installing.md', label: '2' },
    { file: 'docs/03-the-bookshop-trail.md', label: '3' },
  ] },
  { title: 'Part II — TARQL', chapters: [
    { file: 'docs/04-a-first-mapping.md', label: '4' },
    { file: 'docs/05-tarql-in-depth.md', label: '5' },
  ] },
  { title: 'Part III — OTTR', chapters: [
    { file: 'docs/06-templates.md', label: '6' },
    { file: 'docs/07-ottr-over-tables.md', label: '7' },
    { file: 'docs/08-bottr.md', label: '8' },
  ] },
  { title: 'Part IV — Data frames', chapters: [
    { file: 'docs/09-python-and-polars.md', label: '9' },
  ] },
  { title: 'Part V — Checking the transformation', chapters: [
    { file: 'docs/10-shapes-and-shacl.md', label: '10' },
    { file: 'docs/11-sparql-checks.md', label: '11' },
    { file: 'docs/12-the-ontology-quality-suite.md', label: '12' },
  ] },
  { title: 'Part VI — In production', chapters: [
    { file: 'docs/13-into-a-store.md', label: '13' },
    { file: 'docs/14-a-pipeline.md', label: '14' },
  ] },
  { title: 'Appendices', chapters: [
    { file: 'docs/a-command-reference.md', label: 'A' },
    { file: 'docs/b-python-reference.md', label: 'B' },
    { file: 'docs/c-differences.md', label: 'C' },
    { file: 'docs/d-troubleshooting.md', label: 'D' },
    { file: 'docs/e-the-example-files.md', label: 'E' },
  ] },
];

export const CHAPTERS = PARTS.flatMap((p) => p.chapters);
export const FILES = CHAPTERS.map((c) => c.file);

/** Remove the centred logo block at the top of a page. */
export function stripLogo(md) {
  return md.replace(/<p align="center">\s*<img[^>]*>\s*<\/p>\s*/g, '');
}

/** Remove the "| <- prev | next -> |" table at the foot of a chapter; each edition supplies its own. */
export function stripNav(md) {
  return md.replace(/\n---\n+\|[^\n]*\|\n\|[-\s|]+\|\n*$/g, '\n');
}

/** The README as a book's introduction: no title block, contents table or closing mark. */
export function prepareReadme(md, { keepContents = false } = {}) {
  md = md.replace(/<h1 align="center">[\s\S]*?<\/p>\s*---\s*/, '');
  if (!keepContents) md = md.replace(/## How to read it[\s\S]*?(?=\n## )/, '');
  return md.slice(0, md.lastIndexOf('<p align="center">'));
}

/** Read a source file with LF line endings (a checkout with core.autocrlf has CRLF). */
export function readSource(fs, rel) {
  return fs.readFileSync(path.join(MANUAL, rel), 'utf8').replace(/\r\n/g, '\n');
}
