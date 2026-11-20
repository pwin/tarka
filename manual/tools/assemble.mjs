/**
 * The chapters as one book-ready Markdown document, for the PDF and the EPUB: navigation
 * stripped, links between chapters flattened, links into the repository made absolute, and
 * each ```mermaid block replaced by its rendered PNG.
 */
import fs from 'fs';
import path from 'path';
import { CHAPTERS, DIAGRAMS, GITHUB_BLOB, GITHUB_TREE, prepareReadme, readSource, stripLogo, stripNav } from './book.mjs';

/** Text-block width of the A4 page, given 2cm side margins. */
const TEXT_WIDTH_MM = 210 - 2 * 20;
/** Widest a diagram may be before its 15px labels fall below ~8pt on the page. */
const MAX_DIAGRAM_PX = 910;

/**
 * Links between chapters become plain text (the chapter is in the same book) or, for a
 * heading anchor, stay as text too. Links to files in the repository (the examples, the
 * tools) become GitHub links: a reader of the book cannot follow a relative path.
 */
function flattenLinks(md) {
  return md.replace(/\[([^\]]+)\]\(((?!https?:|mailto:)[^)]+)\)/g, (whole, text, target) => {
    const clean = target.replace(/^(\.\.?\/)+/, '');
    if (/^(examples|tools|assets)\//.test(clean)) {
      return `[${text}](${clean.endsWith('/') ? GITHUB_TREE : GITHUB_BLOB}/${clean})`;
    }
    return text;
  });
}

/** Each chapter's book-ready Markdown, in reading order, with the diagrams it uses. */
export function assemble({ diagramPath = (png) => `diagrams/${png}`, sized = true } = {}) {
  const manifest = JSON.parse(fs.readFileSync(path.join(DIAGRAMS, 'manifest.json'), 'utf8'));
  const byKey = new Map(manifest.map((m) => [`${m.rel}#${m.idx}`, m]));
  return CHAPTERS.map((ch) => {
    let md = readSource(fs, ch.file);
    if (ch.file === 'README.md') md = `# Introduction\n\n${prepareReadme(md)}`;
    md = stripNav(stripLogo(md));
    let i = 0;
    md = md.replace(/```mermaid\n[\s\S]*?```/g, () => {
      i += 1;
      const m = byKey.get(`${ch.file}#${i}`);
      if (!m) throw new Error(`no rendered diagram for ${ch.file} #${i}: run render-diagrams.mjs`);
      if (m.w > MAX_DIAGRAM_PX) console.log(`      ! ${m.id} is ${m.w}px wide: its labels will print small`);
      const widthMm = Math.min(TEXT_WIDTH_MM, Math.round((m.w * 25.4) / 96));
      return sized ? `![](${diagramPath(m.png)}){width=${widthMm}mm}` : `![](${diagramPath(m.png)})`;
    });
    return { ...ch, md: flattenLinks(md).trim() };
  });
}
