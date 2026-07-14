import { copyFile, mkdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const webRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const packageRoot = resolve(webRoot, "node_modules/stockfish");
const destination = resolve(webRoot, "public/stockfish");
const assets = [
  ["bin/stockfish-18-lite-single.js", "stockfish-18-lite-single.js"],
  ["bin/stockfish-18-lite-single.wasm", "stockfish-18-lite-single.wasm"],
  ["Copying.txt", "COPYING.txt"],
];

await mkdir(destination, { recursive: true });
await Promise.all(
  assets.map(([source, name]) =>
    copyFile(resolve(packageRoot, source), resolve(destination, name)),
  ),
);
