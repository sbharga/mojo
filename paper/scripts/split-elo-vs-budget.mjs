// Splits elo-vs-budget.mjs's combined output (sweep A / sweep B rows) into
// the two simpler per-sweep CSVs paper/main.tex actually plots/tabulates —
// keeps the LaTeX side to plain, unfiltered pgfplotstable reads instead of
// row-predicate filtering on a mixed-sweep table.
import { readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { repositoryRoot, writeCsv } from './lib.mjs'

const source = readFileSync(resolve(repositoryRoot, 'paper/data/elo-vs-budget.csv'), 'utf8')
const [headerLine, ...lines] = source.trim().split('\n')
const header = headerLine.split(',')
const rows = lines.map((line) => {
  const cells = line.split(',')
  return Object.fromEntries(header.map((key, index) => [key, cells[index]]))
})

const sweepA = rows.filter((row) => row.sweep === 'A')
const sweepB = rows.filter((row) => row.sweep === 'B')

writeFileSync(
  resolve(repositoryRoot, 'paper/data/elo-vs-budget-budget.csv'),
  writeCsv(
    ['budget_ms', 'games', 'mojo_record', 'score_pct', 'perf_elo'],
    sweepA.map((row) => [row.budget_ms, row.games, row.mojo_record, row.score_pct, row.perf_elo]),
  ),
)
writeFileSync(
  resolve(repositoryRoot, 'paper/data/elo-vs-budget-tier.csv'),
  writeCsv(
    ['sf_elo', 'games', 'mojo_record', 'score_pct', 'perf_elo'],
    sweepB.map((row) => [row.sf_elo, row.games, row.mojo_record, row.score_pct, row.perf_elo]),
  ),
)
console.log(`Split into ${sweepA.length} budget rows and ${sweepB.length} tier rows`)
