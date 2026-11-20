/**
 * Build the EPUB edition of the tarka Manual, for e-readers.
 *
 *   node tools/build-epub.mjs [--skip-render]
 *
 * The same book-ready Markdown as the PDF (assemble.mjs), with the diagrams as images that
 * reflow with the text, through pandoc to EPUB 3. Needs pandoc ($PANDOC).
 */
import fs from 'fs';
import path from 'path';
import { execFileSync } from 'child_process';
import { assemble } from './assemble.mjs';
import { AUTHOR, BUILD, DESCRIPTION, MANUAL, SUBTITLE, TITLE } from './book.mjs';

const PANDOC = process.env.PANDOC || 'C:/Program Files/Pandoc/pandoc.exe';
if (!fs.existsSync(PANDOC)) {
  console.error(`missing pandoc: ${PANDOC}\nSet the PANDOC environment variable to its path.`);
  process.exit(1);
}
fs.mkdirSync(BUILD, { recursive: true });
if (!process.argv.includes('--skip-render')) {
  console.log('[1/3] rendering diagrams');
  execFileSync(process.execPath, [path.join(MANUAL, 'tools', 'render-diagrams.mjs')], { stdio: 'inherit' });
}

console.log('[2/3] assembling the book');
// On a small screen, a diagram is as wide as the text, and no wider
const chapters = assemble({ sized: false });
const meta = `---
title:
- type: main
  text: "${TITLE}"
- type: subtitle
  text: "${SUBTITLE}"
creator:
- role: author
  text: "${AUTHOR}"
publisher: "Semantechs"
rights: "MIT licence; © 2026 Peter Winstanley"
lang: en-GB
description: "${DESCRIPTION}"
date: "${new Date().toISOString().slice(0, 10)}"
---

`;
fs.writeFileSync(path.join(BUILD, 'tarka-manual-epub.md'), meta + chapters.map((c) => c.md).join('\n\n'), 'utf8');

console.log('[3/3] rendering the EPUB through pandoc');
const epub = path.join(BUILD, 'The-tarka-Manual.epub');
execFileSync(PANDOC, [
  'tarka-manual-epub.md',
  '--from', 'markdown-smart',
  '--to', 'epub3',
  '--toc', '--toc-depth=2',
  '--split-level=1',
  '--epub-cover-image', path.join(MANUAL, 'assets', 'semantechs-logo-320.png'),
  '--css', path.join(MANUAL, 'tools', 'epub.css'),
  '--syntax-highlighting=none',
  '-o', epub,
], { cwd: BUILD, stdio: 'inherit' });
console.log(`\nwrote ${path.relative(MANUAL, epub)} (${(fs.statSync(epub).size / 1024 / 1024).toFixed(2)} MB)`);
