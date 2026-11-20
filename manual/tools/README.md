# Building the manual

The chapters are Markdown, in `docs/`, with `README.md` as the introduction. These tools build
three editions from them, in the same way as the
[SemOps Manual](https://github.com/pwin/SemOpsManual)'s:

| Edition | Command | Output | Needs |
|---|---|---|---|
| Web, one page per chapter | `node build-site.mjs` | `build/site/` | pandoc, Chromium |
| EPUB 3, for e-readers | `node build-epub.mjs` | `build/The-tarka-Manual.epub` | pandoc, Chromium |
| A4 PDF, for print | `node build-pdf.mjs` | `build/The-tarka-Manual-A4.pdf` | pandoc, typst, Chromium |

```bash
cd manual/tools
npm install
npx puppeteer browsers install chrome-headless-shell   # once
npm run all                                           # diagrams, then the three editions
```

Chromium renders the Mermaid diagrams to PNG (`render-diagrams.mjs`, into `build/diagrams/`);
each build renders them first unless given `--skip-render` (`--skip-diagrams` for the site).
pandoc, typst and Chromium are found at `$PANDOC`, `$TYPST` and `$CHROME_PATH`, with Windows
defaults (`C:/Program Files/Pandoc/pandoc.exe`, `C:/Program Files/typst/typst.exe`, and the
puppeteer cache for Chromium).

`build/` and `node_modules/` are git-ignored, and either may be a link to another drive.

## Publishing

- **Web**: build into the website repository's `tarka_Manual/` directory,
  `node build-site.mjs --out ../../../semantechs/tarka_Manual`, and deploy the site; it is served
  at <https://www.semantechs.co.uk/tarka_Manual/>. The pages make no third-party requests.
- **EPUB and PDF**: attach them to a tarka release, with the version in the file name, rather
  than committing them:

  ```bash
  cp build/The-tarka-Manual.epub build/The-tarka-Manual-v0.3.0.epub
  cp build/The-tarka-Manual-A4.pdf build/The-tarka-Manual-v0.3.0-A4.pdf
  gh release upload v0.3.0 build/The-tarka-Manual-v0.3.0.epub build/The-tarka-Manual-v0.3.0-A4.pdf
  ```

## Files

| File | Does |
|---|---|
| `book.mjs` | the chapters, in order and by part, the title, and the links every edition uses |
| `render-diagrams.mjs` | every ```` ```mermaid ```` block to a PNG at 3× scale, through headless Chromium |
| `assemble.mjs` | the chapters as one book: navigation stripped, links flattened, diagrams substituted |
| `build-site.mjs` | the web edition |
| `build-epub.mjs`, `epub.css` | the EPUB |
| `build-pdf.mjs` | the PDF: pandoc to typst, typst to PDF, with an index of commands, options and functions |
| `typst-template.typ`, `template.typst` | the typst templates (from the SemOps Manual) |
| `site.css` | the web edition's stylesheet (from the SemOps Manual) |

Diagrams follow the SemOps Manual's conventions (its Appendix A): at most about 900 px wide,
labels of at most 28 characters a line, explicit fills from the Semantechs palette.

Every command and output in the chapters comes from `examples/bookshop-trail/`, and
`examples/bookshop-trail/check.py` runs them all; run it before building an edition.
