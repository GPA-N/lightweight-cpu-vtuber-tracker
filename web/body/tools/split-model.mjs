// Splits a model file into parts of at most 50 MB: node split-model.mjs <model.onnx> [outDir]
// Writes <outDir>/<name>.part0, .part1, ... (default outDir: next to the model). The loader takes them as `{ chunks: [urls] }`.
import fs from 'node:fs';
import path from 'node:path';

const PART = 50_000_000;
const [file, outDir = path.dirname(file ?? '.')] = process.argv.slice(2);
if (!file) {
  console.error('usage: node split-model.mjs <model.onnx> [outDir]');
  process.exit(1);
}
const size = fs.statSync(file).size;
const fd = fs.openSync(file, 'r');
const buf = Buffer.alloc(PART);
fs.mkdirSync(outDir, { recursive: true });
for (let n = 0, at = 0; at < size; n++, at += PART) {
  const got = fs.readSync(fd, buf, 0, PART, at);
  const out = path.join(outDir, `${path.basename(file)}.part${n}`);
  fs.writeFileSync(out, buf.subarray(0, got));
  console.log(out, got);
}
fs.closeSync(fd);
