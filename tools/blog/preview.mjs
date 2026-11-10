// Rough local preview of the blog post, for machines without Hugo.
//
// usage: node tools/blog/preview.mjs BLOG_ROOT OUT_DIR [POST.md]
//
// Writes OUT_DIR/index.html and copies the stylesheets, fonts and static files the post
// uses, so that `python -m http.server --directory OUT_DIR` shows something close to the
// real page: the site's own CSS around the post, with the `rawhtml`, `figure` and `splt`
// shortcodes expanded. The Markdown conversion is a small subset (headings, paragraphs,
// lists, tables, emphasis, links). It is for checking layout and the animations, not a
// substitute for a Hugo build.
import { readFileSync, writeFileSync, mkdirSync, cpSync, existsSync } from 'node:fs';
import { join } from 'node:path';

const [blog, out, postArg] = process.argv.slice(2);
if (!blog || !out) { console.error('usage: node tools/blog/preview.mjs BLOG_ROOT OUT_DIR [POST.md]'); process.exit(1); }
const post = postArg || join(blog, 'content/Posts/spl-t-clockwork.md');
let src = readFileSync(post, 'utf8').replace(/\r\n/g, '\n');

const fm = src.match(/^---\n([\s\S]*?)\n---\n/);
const title = (fm[1].match(/^title:\s*"(.*)"\s*$/m) || [, 'Post'])[1];
src = src.slice(fm[0].length).replace(/<!--[\s\S]*?-->/g, '');

const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
const inline = (s) => esc(s)
  .replace(/`([^`]+)`/g, '<code>$1</code>')
  .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
  .replace(/\*([^*]+)\*/g, '<em>$1</em>')
  .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2">$1</a>');
const attrs = (s) => { const a = {}; for (const m of s.matchAll(/(\w+)="([^"]*)"/g)) a[m[1]] = m[2]; return a; };

// Shortcodes become placeholders so the Markdown pass leaves their HTML alone.
const held = [];
const hold = (html) => { held.push(html); return `\n\n@@HELD${held.length - 1}@@\n\n`; };
src = src.replace(/\{\{<\s*rawhtml\s*>\}\}([\s\S]*?)\{\{<\s*\/rawhtml\s*>\}\}/g, (_, inner) => hold(inner));
src = src.replace(/\{\{<\s*figure\s+([\s\S]*?)>\}\}/g, (_, a) => { const p = attrs(a); return hold(`<figure><img src="${p.src}" alt=""><figcaption>${inline(p.caption || '')}</figcaption></figure>`); });
src = src.replace(/\{\{<\s*splt\s+([\s\S]*?)>\}\}/g, (_, a) => {
  const p = attrs(a);
  const d = [`data-games="${p.game || '3898'}"`, `data-from="${p.from || '0'}"`, p.to ? `data-to="${p.to}"` : '', `data-speed="${p.speed || '30'}"`, `data-mode="${p.mode || 'lapse'}"`,
    p.cols ? `data-cols="${p.cols}"` : '', p.labels ? `data-labels="${p.labels}"` : '', p.legend ? `data-legend="${p.legend}"` : '', p.titles ? `data-titles="${p.titles}"` : ''].filter(Boolean).join(' ');
  const img = p.name ? `<img class="splt-poster" src="/splt/${p.name}.svg" alt="${p.alt || ''}" loading="lazy">` : '';
  return hold(`<figure class="splt" ${d}>${img}<figcaption>${inline(p.caption || '')}</figcaption></figure>`);
});

const html = [];
for (const block of src.split(/\n{2,}/)) {
  const b = block.trim();
  if (!b) continue;
  const h = b.match(/^@@HELD(\d+)@@$/);
  if (h) { html.push(held[+h[1]]); continue; }
  const hd = b.match(/^(#{1,4})\s+(.*)$/);
  if (hd) { html.push(`<h${hd[1].length} id="${hd[2].toLowerCase().replace(/[^a-z0-9]+/g, '-')}">${inline(hd[2])}</h${hd[1].length}>`); continue; }
  const lines = b.split('\n');
  if (lines.every((l) => /^- /.test(l))) { html.push('<ul>' + lines.map((l) => `<li>${inline(l.slice(2))}</li>`).join('') + '</ul>'); continue; }
  if (lines.length > 1 && /^\|/.test(lines[0]) && /^\|[-| ]+\|$/.test(lines[1])) {
    const cells = (l) => l.replace(/^\||\|$/g, '').split('|').map((c) => inline(c.trim()));
    html.push('<table><thead><tr>' + cells(lines[0]).map((c) => `<th>${c}</th>`).join('') + '</tr></thead><tbody>' +
      lines.slice(2).map((l) => '<tr>' + cells(l).map((c) => `<td>${c}</td>`).join('') + '</tr>').join('') + '</tbody></table>');
    continue;
  }
  html.push(`<p>${inline(lines.join(' '))}</p>`);
}

const page = `<!DOCTYPE html>
<html lang="en-us"><head>
<title>${esc(title)} (preview)</title>
<meta charset="utf-8"><meta content="width=device-width, initial-scale=1.0" name="viewport">
<link rel="stylesheet" href="https://stackpath.bootstrapcdn.com/bootstrap/4.3.1/css/bootstrap.min.css">
<link rel="stylesheet" href="/css/mero.css"><link rel="stylesheet" href="/css/custom.css">
</head>
<body class="bg-dark m-auto" style="width:95%;">
<div class="wrapper"><div class="content"><div class="page"><div style="max-width:1024px;"><main>
<article class="border rounded bg-dark p-3 position-relative" style="margin-top: 2.5em;">
  <div class="header"><h1 style="font-size: 2.5em;">${esc(title)}</h1></div>
  <div class="ml-auto mr-auto pb-3 pt-3 border-bottom border-top bg-dark"><div class="article">
${html.join('\n')}
  </div></div>
</article>
</main></div></div></div></div>
</body></html>`;

mkdirSync(join(out, 'css'), { recursive: true });
writeFileSync(join(out, 'index.html'), page);
cpSync(join(blog, 'themes/mero/static/css/mero.css'), join(out, 'css/mero.css'));
cpSync(join(blog, 'static/css/custom.css'), join(out, 'css/custom.css'));
for (const dir of ['fonts', 'splt', 'graphs']) if (existsSync(join(blog, 'static', dir))) cpSync(join(blog, 'static', dir), join(out, dir), { recursive: true });
console.log(`wrote ${join(out, 'index.html')} (${html.length} blocks)`);
