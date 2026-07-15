import { copyFileSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const engineDir = fileURLToPath(new URL('.', import.meta.url))
const baselineDir = resolve(engineDir, 'pkg')
const simdDir = mkdtempSync(join(tmpdir(), 'mojo-wasm-simd-'))

function build(outDir, rustFlags = process.env.RUSTFLAGS ?? '') {
  const result = spawnSync(
    'wasm-pack',
    ['build', '--target', 'web', '--out-dir', outDir, '--release'],
    {
      cwd: engineDir,
      env: { ...process.env, RUSTFLAGS: rustFlags },
      stdio: 'inherit',
    },
  )
  if (result.error) throw result.error
  if (result.status !== 0) process.exit(result.status ?? 1)
}

try {
  build(baselineDir)
  build(simdDir, `${process.env.RUSTFLAGS ?? ''} -C target-feature=+simd128`.trim())
  copyFileSync(
    join(simdDir, 'mojo_engine_bg.wasm'),
    join(baselineDir, 'mojo_engine_simd_bg.wasm'),
  )
} finally {
  rmSync(simdDir, { recursive: true, force: true })
}
