// Build the blog's animation bundle and the still images shown where JavaScript does not run.
//
// usage: node tools/blog/build.mjs BLOG_ROOT [POST.md]
//
// Writes BLOG_ROOT/static/splt/splt.js (the games' move lists, the rules engine and the
// player, in that order) and, for every {{< splt ... name="x" ... >}} shortcode in POST.md
// (default: content/Posts/spl-t-clockwork.md), BLOG_ROOT/static/splt/x.svg: the board at the
// shortcode's `at` move (default: its `to`).
//
// Run from the solver repo's root. The games come from runs/best/.
import { createRequire } from 'node:module';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';
const require = createRequire(import.meta.url);
const { parseMoves } = require('./splt-engine.js');
const P = require('./splt-player.js');

const GAMES = { 3898: 'runs/best/game_3898.txt', 3103: 'runs/best/game_3103.txt' };

const blog = process.argv[2];
if (!blog) { console.error('usage: node tools/blog/build.mjs BLOG_ROOT [POST.md]'); process.exit(1); }
const post = process.argv[3] || join(blog, 'content/Posts/spl-t-clockwork.md');
const out = join(blog, 'static/splt');
mkdirSync(out, { recursive: true });

const games = {};
for (const [id, path] of Object.entries(GAMES)) {
  games[id] = parseMoves(readFileSync(path, 'utf8')).map((m) => m.toString(16).padStart(2, '0')).join('');
}
const here = new URL('.', import.meta.url);
const bundle = [
  '/* SPL-T board animations. Built by tools/blog/build.mjs in github.com/jackson-wxyz/spl-t-solver; edit the sources there. */',
  'window.SPLT_GAMES=' + JSON.stringify(games) + ';',
  readFileSync(new URL('splt-engine.js', here), 'utf8'),
  readFileSync(new URL('splt-player.js', here), 'utf8'),
].join('\n');
writeFileSync(join(out, 'splt.js'), bundle);
console.log(`wrote ${join(out, 'splt.js')} (${(bundle.length / 1024).toFixed(1)} KB)`);

// Posters.
export function shortcodes(text) {
  const found = [];
  for (const m of text.matchAll(/\{\{<\s*splt\s+([\s\S]*?)>\}\}/g)) {
    const a = {};
    for (const kv of m[1].matchAll(/(\w+)="([^"]*)"/g)) a[kv[1]] = kv[2];
    found.push(a);
  }
  return found;
}
if (existsSync(post)) {
  let n = 0;
  for (const a of shortcodes(readFileSync(post, 'utf8'))) {
    if (!a.name) continue;
    const ids = (a.game || '3898').split(',');
    const at = parseInt(a.at || a.to || '0', 10);
    const boards = ids.map((id) => { const g = P.getGame(id.trim(), games); return P.boardAt(g, Math.min(at, g.n)); });
    const [c0, c1] = (a.cols || '0-7').split('-').map(Number);
    const svg = P.renderSVG(boards, { c0, c1, cell: c1 - c0 < 2 ? 40 : 22, labels: a.labels !== '0', titles: a.titles ? a.titles.split('|') : null });
    writeFileSync(join(out, a.name + '.svg'), svg);
    n++;
  }
  console.log(`wrote ${n} posters from ${post}`);
} else {
  console.log(`no post at ${post}; posters skipped`);
}
