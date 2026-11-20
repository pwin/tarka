/**
 * Build the web edition of the tarka Manual: one static HTML page per chapter, for serving
 * under https://www.semantechs.co.uk/tarka_Manual/.
 *
 *   node tools/build-site.mjs [--out <dir>] [--skip-diagrams]
 *
 * Output defaults to build/site/. The pages are self-contained static files with no
 * third-party requests (one inline line collapses the contents on phones), so the directory
 * can be copied as it is into the website repository. Diagrams are the PNGs the print edition
 * uses, shown at their natural CSS size. The example files are published under examples/,
 * and viewer.js shows one, highlighted, when its link is clicked. Needs pandoc ($PANDOC),
 * git (for the list of example files) and Chromium (for diagrams).
 */
import fs from 'fs';
import path from 'path';
import { execFileSync } from 'child_process';
import {
  BUILD, CHAPTERS, DESCRIPTION, DIAGRAMS, GITHUB, MANUAL, PARTS, RELEASES, SITE_HOME, SUBTITLE, TITLE,
  prepareReadme, readSource, stripLogo, stripNav,
} from './book.mjs';

const PANDOC = process.env.PANDOC || 'C:/Program Files/Pandoc/pandoc.exe';
const args = process.argv.slice(2);
const outArg = args.indexOf('--out');
const OUT = path.resolve(outArg >= 0 ? args[outArg + 1] : path.join(BUILD, 'site'));

for (const ch of CHAPTERS) {
  ch.page ??= path.basename(ch.file).replace(/\.md$/, '.html');
  if (!ch.short) {
    const h1 = readSource(fs, ch.file).match(/^# (.+)$/m);
    ch.short = h1[1].replace(/^[\dA-Z]+\.\s+/, '').replace(/^Appendix [A-Z]\.\s*/, '');
  }
}
const PAGE_OF = new Map(CHAPTERS.map((c) => [path.basename(c.file), c.page]));

function substituteDiagrams(md, rel, byKey) {
  let i = 0;
  return md.replace(/```mermaid\n[\s\S]*?```/g, () => {
    i += 1;
    const m = byKey.get(`${rel}#${i}`);
    if (!m) throw new Error(`no rendered diagram for ${rel} #${i}: run without --skip-diagrams`);
    return `\n<figure class="diagram"><img src="diagrams/${m.png}" width="${m.w}" height="${m.h}" alt="Diagram" loading="lazy"></figure>\n`;
  });
}

/** The example files' raw GitHub copies, which the EPUB and PDF link to. The web edition
 * publishes its own copies (EXAMPLES below) and links to those, so that viewer.js can show
 * them from the same site. */
const RAW_EXAMPLES = 'https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/';

/** Chapter links become links between pages; links into the repository go to GitHub; links
 * to the example files go to the copies published with the pages. */
function rewriteLinks(html) {
  html = html.split(`href="${RAW_EXAMPLES}`).join('href="examples/');
  return html.replace(/href="((?!https?:|mailto:|#)[^"]+)"/g, (whole, target) => {
    const [file, hash] = target.split('#');
    if (file.startsWith('examples/')) return whole;
    const clean = file.replace(/^(\.\.?\/)+/, '');
    const base = path.basename(clean);
    if (PAGE_OF.has(base)) return `href="${PAGE_OF.get(base)}${hash ? '#' + hash : ''}"`;
    const kind = clean.endsWith('/') ? 'tree' : 'blob';
    return `href="${GITHUB}/${kind}/main/manual/${clean}${hash ? '#' + hash : ''}"`;
  });
}

function toHtml(md) {
  return execFileSync(PANDOC, ['-f', 'gfm', '-t', 'html5', '--syntax-highlighting=none', '--wrap=none'], {
    input: md, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
  });
}

const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

function rail(current) {
  return PARTS.map((part) => {
    const links = part.chapters.map((c) => {
      const cur = c === current ? ' aria-current="page"' : '';
      return `<li><a href="${c.page}"${cur}><span class="n">${c.label}</span><span>${esc(c.short)}</span></a></li>`;
    }).join('\n');
    const head = part.title ? `<p class="part">${esc(part.title)}</p>` : '';
    return `${head}<ol>\n${links}\n</ol>`;
  }).join('\n');
}

function pager(i) {
  const cell = (c, dir) => c
    ? `<a class="${dir}" href="${c.page}"><small>${dir === 'prev' ? '&larr; Previous' : 'Next &rarr;'}</small>${c.label ? c.label + '. ' : ''}${esc(c.short)}</a>`
    : '<span></span>';
  return `<nav class="pager" aria-label="Chapters">${cell(CHAPTERS[i - 1], 'prev')}${cell(CHAPTERS[i + 1], 'next')}</nav>`;
}

function page(ch, i, body) {
  const label = /^[A-E]$/.test(ch.label) ? `Appendix ${ch.label}.` : `${ch.label}.`;
  const title = ch.label ? `${label} ${ch.short} — ${TITLE}` : TITLE;
  const hero = ch.label ? '' : `
    <header class="hero">
      <img src="semantechs-logo-320.png" alt="" width="96" height="96">
      <div>
        <h1>${TITLE}</h1>
        <p><em>${SUBTITLE}: TARQL queries, OTTR templates and data frames, and how to know the RDF is right.</em></p>
      </div>
    </header>`;
  return `<!DOCTYPE html>
<html lang="en-GB">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${esc(title)}</title>
<meta name="description" content="${esc(DESCRIPTION)}">
<link rel="icon" href="semantechs-logo-320.png">
<link rel="stylesheet" href="manual.css">
<script src="viewer.js" defer></script>
</head>
<body>
<header class="topbar">
  <a class="home" href="${SITE_HOME}">&larr; Semantechs home</a>
  <a class="title" href="index.html"><img src="semantechs-logo-320.png" alt="" width="28" height="28">${TITLE}</a>
  <span class="links"><a href="${RELEASES}">EPUB</a><a href="${RELEASES}">A4 PDF</a><a href="${GITHUB}">GitHub</a></span>
</header>
<div class="shell">
  <details class="rail" open>
    <summary>Contents</summary>
    ${rail(ch)}
  </details>
  <script>if (matchMedia('(max-width: 900px)').matches) document.querySelector('.rail').removeAttribute('open');</script>
  <main>
    ${hero}
    <article>
${body}
    </article>
    ${pager(i)}
  </main>
</div>
<footer class="foot">
  <p>MIT — © 2026 Peter Winstanley · <a href="${SITE_HOME}">Semantechs Consulting Ltd</a> · The Semantechs mark is a trademark.</p>
</footer>
</body>
</html>
`;
}

if (!args.includes('--skip-diagrams')) {
  console.log('[1/3] rendering diagrams');
  execFileSync(process.execPath, [path.join(MANUAL, 'tools', 'render-diagrams.mjs')], { stdio: 'inherit' });
}
const manifest = JSON.parse(fs.readFileSync(path.join(DIAGRAMS, 'manifest.json'), 'utf8'));
const byKey = new Map(manifest.map((m) => [`${m.rel}#${m.idx}`, m]));

console.log('[2/3] converting chapters');
fs.rmSync(OUT, { recursive: true, force: true });
fs.mkdirSync(path.join(OUT, 'diagrams'), { recursive: true });
CHAPTERS.forEach((ch, i) => {
  let md = readSource(fs, ch.file);
  if (ch.file === 'README.md') md = prepareReadme(md, { keepContents: true });
  md = substituteDiagrams(stripNav(stripLogo(md)), ch.file, byKey);
  fs.writeFileSync(path.join(OUT, ch.page), page(ch, i, rewriteLinks(toHtml(md))));
});

console.log('[3/3] copying assets');
for (const m of manifest) fs.copyFileSync(path.join(DIAGRAMS, m.png), path.join(OUT, 'diagrams', m.png));
fs.copyFileSync(path.join(MANUAL, 'assets', 'semantechs-logo-320.png'), path.join(OUT, 'semantechs-logo-320.png'));
fs.copyFileSync(path.join(MANUAL, 'tools', 'site.css'), path.join(OUT, 'manual.css'));
fs.copyFileSync(path.join(MANUAL, 'tools', 'viewer.js'), path.join(OUT, 'viewer.js'));
// The example files git tracks (not out/, which may be a junction to another drive), as
// they are in the working tree.
const EXAMPLES = execFileSync('git', ['ls-files', '-z', '--', 'examples'], { cwd: MANUAL, encoding: 'utf8' })
  .split('\0').filter(Boolean);
for (const rel of EXAMPLES) {
  const to = path.join(OUT, rel);
  fs.mkdirSync(path.dirname(to), { recursive: true });
  fs.copyFileSync(path.join(MANUAL, rel), to);
}
console.log(`      ${CHAPTERS.length} pages, ${manifest.length} diagrams, ${EXAMPLES.length} example files -> ${OUT}`);
