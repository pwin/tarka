// The example-file viewer of the tarka Manual's web edition, after the Semantechs
// articles' code.js (the same tokenizer for SPARQL and Turtle, the same dialog).
//
// - A link to a file under examples/ opens it in a dialog instead of as raw text:
//   highlighted (SPARQL, Turtle, stOTTR, Python, YAML, Markdown), a CSV file as a
//   table, a Parquet file as a note with its download. The dialog's buttons open the
//   file in the Turtle Editor Viewer (when the file's row in the page has an editor
//   link), copy it, or download it. Ctrl/Cmd/Shift-click, or a middle click, still
//   opens the raw file.
// - The file is fetched from this site; only if the site cannot serve it is it
//   fetched from the copy on GitHub that the EPUB and PDF link to.
//
// No dependencies.
(function () {
  'use strict';

  var GITHUB_RAW = 'https://raw.githubusercontent.com/pwin/tarka/main/manual/';

  var esc = function (s) {
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  };
  var span = function (cls, text) { return '<span class="hl-' + cls + '">' + esc(text) + '</span>'; };

  // ------------------------------------------------------------ SPARQL, Turtle and stOTTR

  var KEYWORDS = new RegExp('^(?:' + [
    'BASE', 'PREFIX', 'SELECT', 'CONSTRUCT', 'DESCRIBE', 'ASK', 'WHERE', 'FROM', 'NAMED',
    'DISTINCT', 'REDUCED', 'AS', 'FILTER', 'OPTIONAL', 'UNION', 'MINUS', 'GRAPH', 'SERVICE',
    'SILENT', 'BIND', 'VALUES', 'UNDEF', 'GROUP', 'BY', 'HAVING', 'ORDER', 'ASC', 'DESC',
    'LIMIT', 'OFFSET', 'NOT', 'EXISTS', 'IN', 'INSERT', 'DELETE', 'DATA', 'WITH', 'USING',
    'LOAD', 'CLEAR', 'DROP', 'CREATE', 'ADD', 'MOVE', 'COPY', 'INTO', 'DEFAULT', 'ALL', 'VERSION'
  ].join('|') + ')$', 'i');
  var OTTR_WORDS = /^(?:cross|zipMin|zipMax|none)$/;

  // Ordered: the first pattern that matches at the current position wins.
  var RDF_RULES = [
    ['ws', /\s+/y],
    ['c', /#[^\n]*/y],
    ['s3', /"""[\s\S]*?"""|'''[\s\S]*?'''/y],
    ['s', /"(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*'/y],
    ['punct', /<<\(|\)>>|<<|>>|\{\||\|\}|::|\+\+/y],
    ['i', /<[^<>"{}|^`\\\s]*>/y],
    ['k', /@(?:prefix|base)\b/y],
    ['t', /@[A-Za-z]+(?:-[A-Za-z0-9]+)*(?:--(?:ltr|rtl))?/y],
    ['v', /[?$][A-Za-z_À-￿][\wÀ-￿]*/y],
    ['t', /\^\^/y],
    ['p', /(?:[A-Za-zÀ-￿][\w.\-À-￿]*)?:(?:[\wÀ-￿%](?:[\w.\-À-￿%]*[\w\-À-￿%])?)?/y],
    ['n', /[+-]?(?:\d+\.\d*(?:[eE][+-]?\d+)?|\.\d+(?:[eE][+-]?\d+)?|\d+(?:[eE][+-]?\d+)?)/y],
    ['w', /[A-Za-z_][\w]*/y],
    ['other', /[\s\S]/y],
  ];

  function tokens(text, rules, each) {
    var pos = 0;
    while (pos < text.length) {
      var kind = null, m = null;
      for (var r = 0; r < rules.length; r++) {
        rules[r][1].lastIndex = pos;
        m = rules[r][1].exec(text);
        if (m && m[0].length) { kind = rules[r][0]; break; }
      }
      pos += m[0].length;
      each(kind, m[0], pos);
    }
  }

  function rdf(text, lang) {
    var out = '';
    var sparql = lang === 'sparql';
    tokens(text, RDF_RULES, function (kind, tok, pos) {
      if (kind === 'ws' || kind === 'other' || kind === 'punct') { out += esc(tok); return; }
      if (kind === 's3') {
        // A long string holding a query (sh:select, sh:construct) is highlighted as SPARQL.
        var q = tok.slice(0, 3), inner = tok.slice(3, -3);
        if (!sparql && /^\s*(?:PREFIX|SELECT|CONSTRUCT|ASK|DESCRIBE)\b/i.test(inner)) out += span('s', q) + rdf(inner, 'sparql') + span('s', q);
        else out += span('s', tok);
        return;
      }
      if (kind === 'w') {
        var call = /^\s*\(/.test(text.slice(pos));
        if (tok === 'a' || tok === 'true' || tok === 'false') out += span(tok === 'a' ? 'k' : 'n', tok);
        else if (KEYWORDS.test(tok) && (sparql || /^(?:PREFIX|BASE)$/i.test(tok))) out += span('k', tok);
        else if (lang === 'stottr' && OTTR_WORDS.test(tok)) out += span('k', tok);
        else if (call) out += span('f', tok);
        else out += esc(tok);
        return;
      }
      out += span(kind, tok);
    });
    return out;
  }

  // ------------------------------------------------------------ Python

  var PY_KEYWORDS = /^(?:and|as|assert|async|await|break|class|continue|def|del|elif|else|except|finally|for|from|global|if|import|in|is|lambda|match|case|nonlocal|not|or|pass|raise|return|try|while|with|yield)$/;
  var PY_CONSTANTS = /^(?:True|False|None)$/;
  var PY_RULES = [
    ['ws', /\s+/y],
    ['c', /#[^\n]*/y],
    ['s3', /[rRbBuUfF]{0,2}(?:"""[\s\S]*?"""|'''[\s\S]*?''')/y],
    ['s', /[rRbBuUfF]{0,2}(?:"(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*')/y],
    ['t', /@[A-Za-z_][\w.]*/y],
    ['n', /\b(?:0[xX][\da-fA-F_]+|\d[\d_]*(?:\.\d[\d_]*)?(?:[eE][+-]?\d+)?j?)\b/y],
    ['w', /[A-Za-z_][\w]*/y],
    ['other', /[\s\S]/y],
  ];

  function python(text) {
    var out = '', prev = '';
    tokens(text, PY_RULES, function (kind, tok, pos) {
      if (kind === 'ws' || kind === 'other') { out += esc(tok); if (kind === 'other') prev = tok; return; }
      if (kind === 'w') {
        if (PY_KEYWORDS.test(tok)) out += span('k', tok);
        else if (PY_CONSTANTS.test(tok)) out += span('n', tok);
        else if (prev === 'def' || prev === 'class' || /^\s*\(/.test(text.slice(pos))) out += span('f', tok);
        else out += esc(tok);
        prev = tok;
        return;
      }
      out += span(kind === 's3' ? 's' : kind, tok);
      prev = tok;
    });
    return out;
  }

  // ------------------------------------------------------------ YAML

  var YAML_VALUE = [
    ['ws', /[ \t]+/y],
    ['c', /#.*/y],
    ['v', /\$\{\{[\s\S]*?\}\}/y],
    ['s', /"(?:[^"\\]|\\.)*"|'(?:[^']|'')*'/y],
    ['n', /(?:true|false|null|~|[+-]?\d+(?:\.\d+)?)(?=\s*(?:#|$))/y],
    ['other', /[^\s#$"']+|[\s\S]/y],
  ];

  function yaml(text) {
    return text.split('\n').map(function (line) {
      var m = line.match(/^(\s*)(- )?((?:[\w.\-/]+|"[^"]*"|'[^']*'))(:)(?=\s|$)(.*)$/);
      var lead = '', rest = line;
      if (m) {
        lead = esc(m[1]) + (m[2] ? span('t', '-') + ' ' : '') + span('p', m[3]) + esc(m[4]);
        rest = m[5];
      } else if ((m = line.match(/^(\s*)(- )(.*)$/))) {
        lead = esc(m[1]) + span('t', '-') + ' ';
        rest = m[3];
      }
      var out = '';
      tokens(rest, YAML_VALUE, function (kind, tok) {
        out += kind === 'ws' || kind === 'other' ? esc(tok) : span(kind, tok);
      });
      return lead + out;
    }).join('\n');
  }

  // ------------------------------------------------------------ Markdown

  function markdown(text) {
    return text.split('\n').map(function (line) {
      if (/^#{1,6}\s/.test(line)) return span('k', line);
      return esc(line)
        .replace(/`([^`]+)`/g, '<span class="hl-s">`$1`</span>')
        .replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, '[$1](<span class="hl-i">$2</span>)')
        .replace(/(https?:\/\/[^\s<)]+)(?![^<]*<\/span>)/g, '<span class="hl-i">$1</span>');
    }).join('\n');
  }

  // ------------------------------------------------------------ CSV, as a table

  // RFC 4180: quoted fields, doubled quotes, commas and line breaks inside quotes.
  function parseCsv(text) {
    var rows = [], row = [], field = '', i = 0, quoted = false;
    text = text.replace(/\r\n/g, '\n');
    while (i < text.length) {
      var ch = text[i];
      if (quoted) {
        if (ch === '"' && text[i + 1] === '"') { field += '"'; i += 2; continue; }
        if (ch === '"') { quoted = false; i++; continue; }
        field += ch; i++; continue;
      }
      if (ch === '"' && field === '') { quoted = true; i++; continue; }
      if (ch === ',') { row.push(field); field = ''; i++; continue; }
      if (ch === '\n') { row.push(field); rows.push(row); row = []; field = ''; i++; continue; }
      field += ch; i++;
    }
    if (field !== '' || row.length) { row.push(field); rows.push(row); }
    return rows;
  }

  function csvTable(text) {
    var rows = parseCsv(text);
    if (!rows.length) return { html: '<p>An empty file.</p>', summary: 'empty' };
    var head = rows[0], body = rows.slice(1);
    var html = '<div class="cv-table-wrap"><table class="cv-table"><thead><tr><th class="cv-rownum">#</th>' +
      head.map(function (h) { return '<th>' + esc(h) + '</th>'; }).join('') + '</tr></thead><tbody>' +
      body.map(function (r, n) {
        return '<tr><td class="cv-rownum">' + (n + 1) + '</td>' + head.map(function (_, c) {
          var v = r[c] === undefined ? '' : r[c];
          return v === '' ? '<td class="cv-empty"></td>' : '<td>' + esc(v) + '</td>';
        }).join('') + '</tr>';
      }).join('') + '</tbody></table></div>';
    return { html: html, summary: body.length + (body.length === 1 ? ' row' : ' rows') + ', ' + head.length + ' columns' };
  }

  // ------------------------------------------------------------ the kinds of file

  var KINDS = [
    [/\.rq$/i, 'SPARQL', function (t) { return rdf(t, 'sparql'); }],
    [/\.bottr\.ttl$/i, 'bOTTR · Turtle', function (t) { return rdf(t, 'turtle'); }],
    [/\.ttl$/i, 'Turtle', function (t) { return rdf(t, 'turtle'); }],
    [/\.stottr$/i, 'stOTTR', function (t) { return rdf(t, 'stottr'); }],
    [/\.py$/i, 'Python', python],
    [/\.ya?ml$/i, 'YAML', yaml],
    [/\.md$/i, 'Markdown', markdown],
    [/\.csv$/i, 'CSV', null],
    [/\.parquet$/i, 'Parquet', null],
  ];
  var kindOf = function (name) { return KINDS.filter(function (k) { return k[0].test(name); })[0]; };

  // ------------------------------------------------------------ the dialog

  var dialog = null;

  function build() {
    dialog = document.createElement('dialog');
    dialog.className = 'codeview';
    dialog.setAttribute('aria-labelledby', 'cv-title');
    dialog.innerHTML =
      '<div class="cv-head">' +
        '<div class="cv-titles"><span class="cv-id"></span><h2 id="cv-title"></h2></div>' +
        '<div class="cv-actions">' +
          '<a class="cv-btn cv-open" target="_blank" rel="noopener">Open in editor</a>' +
          '<button type="button" class="cv-btn cv-copy">Copy</button>' +
          '<a class="cv-btn cv-download">Download</a>' +
          '<button type="button" class="cv-close" aria-label="Close">&times;</button>' +
        '</div>' +
      '</div>' +
      '<div class="cv-body" tabindex="-1">' +
        '<p class="cv-lede"></p>' +
        '<p class="cv-meta"></p>' +
        '<div class="cv-content"></div>' +
      '</div>';
    document.body.appendChild(dialog);
    dialog.querySelector('.cv-close').addEventListener('click', function () { dialog.close(); });
    // A click on the backdrop (outside the dialog box) closes it.
    dialog.addEventListener('click', function (e) {
      var r = dialog.getBoundingClientRect();
      if (e.target === dialog && (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom)) dialog.close();
    });
    dialog.querySelector('.cv-copy').addEventListener('click', function () {
      var btn = this, text = dialog._text;
      var done = function (label) { btn.textContent = label; setTimeout(function () { btn.textContent = 'Copy'; }, 1600); };
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text).then(function () { done('Copied'); }, function () { done('Copy failed'); });
      } else done('Copy failed');
    });
  }

  // What the page says about the file: the rest of its table row (what it is, the
  // chapters) and the row's link into the Turtle Editor Viewer, if it has one.
  function context(a) {
    var none = { about: '', chapters: '', editor: '', editorResult: '' };
    var row = a.closest('tr');
    if (!row) return none;
    var cells = Array.prototype.slice.call(row.children);
    var own = a.closest('td, th');
    var editor = row.querySelector('a[href*="turtle-editor-viewer/"]');
    var headers = Array.prototype.map.call(row.closest('table').querySelectorAll('thead th'), function (th) { return th.textContent.trim(); });
    var about = '', chapters = '';
    cells.forEach(function (c, i) {
      if (c === own) return;
      var label = headers[i] || '';
      // a bOTTR map's description ends "(open)": the editor link, which has its own button
      var t = c.textContent.replace(/\s+/g, ' ').replace(/\s*\(open\)\s*$/, '').trim();
      if (/^Chapters?$/.test(label)) chapters = t;
      else if (label !== 'In the editor' && t && !about) about = t;
    });
    return { about: about, chapters: chapters, editor: editor ? editor.href : '', editorResult: editor ? editorNote(editor) : '' };
  }

  // The result the page writes beside an editor link ("open: six warnings, no violations").
  function editorNote(link) {
    var cell = link.closest('td');
    var t = cell ? cell.textContent.replace(/\s+/g, ' ').trim() : '';
    var m = t.match(/^open\s*:\s*(.+)$/);
    return m ? m[1] : '';
  }

  // The text of the first address that serves it, and which one that was.
  function fetchText(urls) {
    var tryAt = function (i) {
      return fetch(urls[i]).then(function (r) {
        if (!r.ok) throw new Error(r.status);
        return r.text();
      }).then(function (text) { return { text: text, from: i }; }, function (e) {
        if (i + 1 < urls.length) return tryAt(i + 1);
        throw e;
      });
    };
    return tryAt(0);
  }

  function show(path, url, text, ctx, local) {
    if (!dialog) build();
    var $ = function (s) { return dialog.querySelector(s); };
    var name = path.split('/').pop();
    var kind = kindOf(name);
    var label = kind ? kind[1] : 'Text';

    $('#cv-title').textContent = path;
    $('.cv-lede').textContent = ctx.about;
    $('.cv-lede').hidden = !ctx.about;

    var meta = [];
    var content = $('.cv-content');
    if (label === 'CSV') {
      var table = csvTable(text);
      content.innerHTML = table.html;
      meta.push('<span class="cv-label">Table</span> ' + esc(table.summary));
    } else if (label === 'Parquet') {
      content.innerHTML = '<p class="cv-note">A Parquet file is binary, so it is not shown here. It holds the same table as ' +
        '<code>shop-stock.csv</code>, with its four list columns as lists and its copies and prices typed. Download it, ' +
        'and read it with Polars: <code>pl.read_parquet("shop-stock.parquet")</code>, or give it to ' +
        '<code>tarka run -i</code>.</p>';
    } else {
      var lines = text.replace(/\s+$/, '').split('\n').length;
      content.innerHTML = '<pre class="cv-code"><code></code></pre>';
      $('.cv-code code').innerHTML = (kind && kind[2] ? kind[2] : esc)(text.replace(/\r\n/g, '\n').replace(/\s+$/, ''));
      meta.push('<span class="cv-label">Lines</span> ' + lines);
    }
    $('.cv-id').textContent = label;
    if (ctx.chapters) meta.push('<span class="cv-label">' + (/[,\s]|to/.test(ctx.chapters) ? 'Chapters' : 'Chapter') + '</span> ' + esc(ctx.chapters));
    if (ctx.editorResult) meta.push('<span class="cv-label">In the editor</span> ' + esc(ctx.editorResult));
    $('.cv-meta').innerHTML = meta.join('<span class="cv-sep">·</span>');
    $('.cv-meta').hidden = !meta.length;

    var open = $('.cv-open');
    open.hidden = !ctx.editor;
    if (ctx.editor) open.href = ctx.editor;

    var copy = $('.cv-copy');
    copy.hidden = label === 'Parquet';
    dialog._text = text;

    // Download the site's copy; if the site could not serve it, the text as fetched.
    var download = $('.cv-download');
    if (dialog._blob) { URL.revokeObjectURL(dialog._blob); dialog._blob = null; }
    if (local) download.href = url;
    else download.href = dialog._blob = URL.createObjectURL(new Blob([text], { type: 'text/plain;charset=utf-8' }));
    download.setAttribute('download', name);

    $('.cv-body').scrollTop = 0;
    dialog.showModal();
    // Focus the content rather than a button, so the arrow keys scroll it and no focus
    // ring is drawn on opening; Tab still reaches every button, and Escape closes.
    $('.cv-body').focus();
  }

  function onClick(e) {
    if (e.defaultPrevented || e.button !== 0 || e.ctrlKey || e.metaKey || e.shiftKey || e.altKey) return;
    var a = e.target.closest && e.target.closest('article a[href]');
    if (!a) return;
    var url;
    try { url = new URL(a.getAttribute('href'), location.href); } catch (err) { return; }
    var at = url.pathname.indexOf('/examples/');
    if (url.origin !== location.origin || at < 0 || !kindOf(url.pathname)) return;
    e.preventDefault();
    var rel = decodeURIComponent(url.pathname.slice(at + 1));            // examples/bookshop-trail/…
    var path = rel.replace(/^examples\/bookshop-trail\//, '');
    var ctx = context(a);
    if (/\.parquet$/i.test(path)) { show(path, url.href, '', ctx, true); return; }
    fetchText([url.href, GITHUB_RAW + rel]).then(function (got) {
      show(path, url.href, got.text, ctx, got.from === 0);
    }).catch(function () { location.href = url.href; });
  }

  function start() {
    if (typeof HTMLDialogElement === 'function') document.addEventListener('click', onClick);
  }

  // For testing and for the console: TarkaViewer.highlight(text, 'stottr'), .show(…).
  window.TarkaViewer = {
    highlight: function (text, lang) {
      return lang === 'python' ? python(text) : lang === 'yaml' ? yaml(text) : lang === 'markdown' ? markdown(text) : rdf(text, lang);
    },
    parseCsv: parseCsv,
  };

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start);
  else start();
})();
