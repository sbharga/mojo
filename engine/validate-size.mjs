import { readFileSync } from 'node:fs'
import { brotliCompressSync, constants, gzipSync } from 'node:zlib'

const limits = {
  baseline: { path: './pkg/mojo_engine_bg.wasm', gzip: 230_000 },
  simd: { path: './pkg/mojo_engine_simd_bg.wasm', gzip: 230_000 },
}
const report = {}
for (const [name, limit] of Object.entries(limits)) {
  const bytes = readFileSync(new URL(limit.path, import.meta.url))
  const gzip = gzipSync(bytes).byteLength
  const brotli = brotliCompressSync(bytes, {
    params: { [constants.BROTLI_PARAM_QUALITY]: 11 },
  }).byteLength
  report[name] = { raw: bytes.byteLength, gzip, brotli, gzip_budget: limit.gzip }
  if (gzip > limit.gzip) {
    throw new Error(`${name} Wasm gzip size ${gzip} exceeds ${limit.gzip}`)
  }
}
console.log(report)
