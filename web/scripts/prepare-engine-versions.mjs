import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const webRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const repositoryRoot = resolve(webRoot, '..')
const outputRoot = resolve(webRoot, 'public', 'engine-versions')
const wasmPath = 'engine/pkg/mojo_engine_bg.wasm'
const modulePath = 'engine/pkg/mojo_engine.js'
const requiredModuleFragments = [
  'export class Engine',
  'analyze_depth(',
  'fallback_move()',
  'set_position(',
  'free()',
  'export { initSync, __wbg_init as default }',
]

function git(args, encoding) {
  return execFileSync('git', args, {
    cwd: repositoryRoot,
    encoding,
    maxBuffer: 128 * 1024 * 1024,
  })
}

const history = git([
  'log',
  '--format=%H%x1f%h%x1f%aI%x1f%s%x1e',
  '--',
  wasmPath,
], 'utf8')

const candidates = history
  .split('\x1e')
  .map((record) => record.trim())
  .filter(Boolean)
  .map((record) => {
    const [sha, shortSha, committedAt, subject] = record.split('\x1f')
    if (!/^[0-9a-f]{40}$/.test(sha)) throw new Error(`Invalid engine commit SHA: ${sha}`)
    return { sha, shortSha, committedAt, subject }
  })

const versions = candidates.flatMap((candidate) => {
  const moduleSource = git(['show', `${candidate.sha}:${modulePath}`], 'utf8')
  const missing = requiredModuleFragments.filter((fragment) => !moduleSource.includes(fragment))
  if (missing.length > 0) {
    console.warn(`Skipping incompatible engine build ${candidate.shortSha}: missing ${missing.join(', ')}`)
    return []
  }
  return [{
    ...candidate,
    modulePath: `${candidate.sha}/mojo_engine.js`,
    wasmPath: `${candidate.sha}/mojo_engine_bg.wasm`,
  }]
})

if (versions.length < 2) throw new Error(`At least two compatible builds are required from ${wasmPath}`)

rmSync(outputRoot, { recursive: true, force: true })
mkdirSync(outputRoot, { recursive: true })

for (const version of versions) {
  const versionRoot = resolve(outputRoot, version.sha)
  mkdirSync(versionRoot, { recursive: true })
  writeFileSync(resolve(versionRoot, 'mojo_engine.js'), git(['show', `${version.sha}:${modulePath}`]))
  writeFileSync(resolve(versionRoot, 'mojo_engine_bg.wasm'), git(['show', `${version.sha}:${wasmPath}`]))
}

writeFileSync(
  resolve(outputRoot, 'manifest.json'),
  `${JSON.stringify({ generatedAt: new Date().toISOString(), versions }, null, 2)}\n`,
)
copyFileSync(resolve(repositoryRoot, 'engine', 'openings.json'), resolve(outputRoot, 'openings.json'))

console.log(`Prepared ${versions.length} historical engine versions`)
