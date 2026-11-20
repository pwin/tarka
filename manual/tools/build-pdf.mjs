/**
 * Build the A4 print edition of the tarka Manual.
 *
 *   node tools/build-pdf.mjs [--skip-render]
 *
 *   1. render   - each ```mermaid block to a high-DPI PNG (render-diagrams.mjs)
 *   2. assemble - the chapters as one print-ready Markdown file (assemble.mjs)
 *   3. index    - an index of commands, options and functions, from the source
 *   4. render   - pandoc to typst, typst to PDF
 *
 * Needs pandoc ($PANDOC) and typst ($TYPST); both are checked first.
 */
import fs from 'fs';
import path from 'path';
import { execFileSync } from 'child_process';
import { assemble } from './assemble.mjs';
import { AUTHOR, BUILD, MANUAL, SUBTITLE, TITLE } from './book.mjs';

const PANDOC = process.env.PANDOC || 'C:/Program Files/Pandoc/pandoc.exe';
const TYPST = process.env.TYPST || 'C:/Program Files/typst/typst.exe';

const pageBreak = () => '\n\n```{=typst}\n#pagebreak(weak: true)\n```\n\n';

/**
 * Entries reference chapters, not pages: they are resolved from the source, so they stay
 * right as the manual is revised.
 */
function buildIndex(chapters) {
  const hits = new Map(); // term -> { kind, chapters }
  const add = (term, kind, label) => {
    if (!hits.has(term)) hits.set(term, { kind, chapters: new Set() });
    hits.get(term).chapters.add(label || 'Introduction');
  };
  for (const ch of chapters) {
    const body = ch.md;
    for (const c of ['run', 'expand', 'bottr', 'shapes', 'lint']) {
      if (new RegExp(`\\btarka ${c}\\b`).test(body)) add(`tarka ${c}`, 'command', ch.label);
    }
    for (const f of body.match(/(?<![\w-])--[a-z][a-z0-9-]{2,}/g) || []) add(f, 'option', ch.label);
    for (const f of body.match(/\b(?:apf|afn|fn|spif|geof):[a-zA-Z][\w-]*/g) || []) add(f, 'function', ch.label);
    for (const f of body.match(/\bottr:[a-zA-Z][\w]*/g) || []) add(f, 'ottr', ch.label);
  }
  const rank = (x) => (x === 'Introduction' ? -1 : /^\d+$/.test(x) ? Number(x) : 100 + x.charCodeAt(0));
  const group = (kind, heading, note) => {
    const rows = [...hits.entries()].filter(([, v]) => v.kind === kind)
      .sort((a, b) => a[0].replace(/^-+/, '').localeCompare(b[0].replace(/^-+/, '')));
    if (!rows.length) return '';
    let out = `\n## ${heading}\n\n${note ? note + '\n\n' : ''}| Entry | Chapters |\n|---|---|\n`;
    for (const [term, v] of rows) out += `| \`${term}\` | ${[...v.chapters].sort((a, b) => rank(a) - rank(b)).join(', ')} |\n`;
    return out;
  };
  console.log(`      ${hits.size} index entries`);
  return `# Index\n\nEntries refer to chapters (A to E are the appendices).\n`
    + group('command', 'Commands')
    + group('option', 'Options')
    + group('function', 'Functions', 'Jena (`apf:`, `afn:`), XPath (`fn:`), SPIN (`spif:`) and GeoSPARQL (`geof:`).')
    + group('ottr', 'OTTR and bOTTR terms');
}

for (const [what, p] of [['pandoc', PANDOC], ['typst', TYPST]]) {
  if (!fs.existsSync(p)) {
    console.error(`missing ${what}: ${p}\nSet the ${what.toUpperCase()} environment variable to its path.`);
    process.exit(1);
  }
}
fs.mkdirSync(BUILD, { recursive: true });
if (!process.argv.includes('--skip-render')) {
  console.log('[1/4] rendering diagrams');
  execFileSync(process.execPath, [path.join(MANUAL, 'tools', 'render-diagrams.mjs')], { stdio: 'inherit' });
}
console.log('[2/4] assembling print-ready Markdown');
const chapters = assemble();
console.log('[3/4] generating the index');
const indexMd = buildIndex(chapters);

const date = new Date().toISOString().slice(0, 10);
const meta = `---
title: "${TITLE}"
subtitle: "${SUBTITLE}"
author: "${AUTHOR}"
date: "${date}"
papersize: a4
margin:
  x: 20mm
  y: 20mm
mainfont: "Cambria"
codefont: "Consolas"
fontsize: 10pt
linestretch: 1.1
linkcolor: "#1a5f7a"
logo: "logo.png"
toc-depth: 2
---

`;
const body = chapters.map((c) => c.md).join(pageBreak()) + pageBreak() + indexMd;
fs.writeFileSync(path.join(BUILD, 'tarka-manual.md'), meta + body, 'utf8');
// typst refuses paths outside --root, so the logo is copied in.
fs.copyFileSync(path.join(MANUAL, 'assets', 'semantechs-logo-320.png'), path.join(BUILD, 'logo.png'));

console.log('[4/4] rendering the PDF through pandoc and typst');
// Two steps rather than --pdf-engine: pandoc's own PDF path copies images to an absolute
// temporary directory, which typst rejects; a .typ file keeps them relative to build/.
execFileSync(PANDOC, [
  'tarka-manual.md',
  // pandoc's markdown, for {width=…}; -smart keeps "--flag" from becoming an en dash
  '--from', 'markdown-smart',
  '--to', 'typst',
  '--standalone',
  '--toc',
  '--template', path.join(MANUAL, 'tools', 'typst-template.typ'),
  '-o', 'tarka-manual.typ',
], { cwd: BUILD, stdio: 'inherit' });
// Centre the diagrams, which pandoc places hard against the left margin.
const typPath = path.join(BUILD, 'tarka-manual.typ');
fs.writeFileSync(typPath, fs.readFileSync(typPath, 'utf8').replace(/#box\(image\((.*?)\)\)/g, '#align(center, image($1))'), 'utf8');
const pdfPath = path.join(BUILD, 'The-tarka-Manual-A4.pdf');
execFileSync(TYPST, ['compile', '--root', '.', 'tarka-manual.typ', pdfPath], { cwd: BUILD, stdio: 'inherit' });
console.log(`\nwrote ${path.relative(MANUAL, pdfPath)} (${(fs.statSync(pdfPath).size / 1024 / 1024).toFixed(2)} MB)`);
