// Check tools/blog/splt-engine.js against the Rust engine, position by position.
//
// usage: node tools/blog/verify.mjs GAME.txt FRAMES.txt [GAME.txt FRAMES.txt ...]
//
// FRAMES.txt is the output of `frames GAME.txt` (src/bin/frames.rs): the board after every
// move. Exits with status 1 at the first position that differs.
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
const require = createRequire(import.meta.url);
const { Board, parseMoves } = require('./splt-engine.js');

const args = process.argv.slice(2);
let total = 0;
for (let a = 0; a + 1 < args.length; a += 2) {
  const moves = parseMoves(readFileSync(args[a], 'utf8'));
  const frames = readFileSync(args[a + 1], 'utf8').split(/\r?\n/).filter((l) => l.length);
  if (frames.length !== moves.length + 1) {
    console.error(`${args[a]}: ${moves.length} moves but ${frames.length} frames`);
    process.exit(1);
  }
  const b = new Board();
  if (b.toLine() !== frames[0]) { console.error(`${args[a]}: start position differs`); process.exit(1); }
  for (let k = 0; k < moves.length; k++) {
    if (!b.play(moves[k])) { console.error(`${args[a]}: move ${k + 1} (block ${moves[k]}) is illegal here`); process.exit(1); }
    if (b.toLine() !== frames[k + 1]) {
      console.error(`${args[a]}: position after move ${k + 1} differs\n  js:   ${b.toLine()}\n  rust: ${frames[k + 1]}`);
      process.exit(1);
    }
  }
  const over = b.legalMoves().length === 0;
  console.log(`${args[a]}: ${moves.length} positions identical; game over at the end: ${over}`);
  total += moves.length;
}
console.log(`all ${total} positions identical`);
