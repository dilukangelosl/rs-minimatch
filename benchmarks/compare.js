// Throwaway comparison harness: rs-minimatch (this repo's Node bindings)
// vs the real `minimatch` npm package, doing the same operations over the
// same generated data as crates/core/benches/core_benches.rs. Not a
// rigorous benchmark suite (no criterion-style statistics, single machine,
// single process) - just a reproducible, honest side-by-side. Run with:
//
//   cd packages/rs-minimatch && npm run build && cd ../../benchmarks
//   npm install && node compare.js
'use strict'

const rsMinimatch = require('../packages/rs-minimatch')
const { minimatch: nodeMinimatch, match: nodeMatch } = require('minimatch')

function mulberry32(seed) {
  let a = seed
  return function () {
    a |= 0
    a = (a + 0x6d2b79f5) | 0
    let t = Math.imul(a ^ (a >>> 15), 1 | a)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

function randInt(rng, max) {
  return Math.floor(rng() * max)
}

function randomPath(rng) {
  const dirs = ['src', 'lib', 'test', 'node_modules', 'dist', 'components']
  const exts = ['js', 'ts', 'json', 'css', 'md']
  const depth = 1 + randInt(rng, 4)
  const parts = []
  for (let i = 0; i < depth; i++) {
    parts.push(dirs[randInt(rng, dirs.length)])
  }
  parts.push(`file${randInt(rng, 1000)}.${exts[randInt(rng, exts.length)]}`)
  return parts.join('/')
}

function timeMs(fn) {
  const start = process.hrtime.bigint()
  fn()
  const end = process.hrtime.bigint()
  return Number(end - start) / 1e6
}

function bestOf(runs, fn) {
  let best = Infinity
  for (let i = 0; i < runs; i++) {
    const t = timeMs(fn)
    if (t < best) best = t
  }
  return best
}

const RUNS = 5
const results = []

function bench(name, fn) {
  fn()
  const ms = bestOf(RUNS, fn)
  results.push({ name, ms })
  console.log(`${name.padEnd(32)} ${ms.toFixed(2).padStart(10)} ms`)
}

// --- match 10k paths against a pattern ---
{
  const rng = mulberry32(42)
  const paths = Array.from({ length: 10_000 }, () => randomPath(rng))
  bench('rs-minimatch: match 10k', () => {
    for (const p of paths) rsMinimatch(p, 'src/**/*.js')
  })
  bench('minimatch: match 10k', () => {
    for (const p of paths) nodeMinimatch(p, 'src/**/*.js')
  })
}

// --- compile 1k patterns ---
{
  const rng = mulberry32(7)
  const dirs = ['src', 'lib', 'test']
  const patterns = Array.from({ length: 1_000 }, () => `${dirs[randInt(rng, dirs.length)]}/**/*.{js,ts}`)
  bench('rs-minimatch: compile 1k', () => {
    for (const p of patterns) new rsMinimatch.Minimatch(p)
  })
  bench('minimatch: compile 1k', () => {
    for (const p of patterns) new (require('minimatch').Minimatch)(p)
  })
}

// --- the PRD's own attack shape: 11 chained globstars over 30 segments ---
{
  const pattern = '**/'.repeat(11) + 'foo'
  const path = 'a/'.repeat(30) + 'foo'
  bench('rs-minimatch: globstar attack shape', () => {
    rsMinimatch(path, pattern)
  })
  bench('minimatch: globstar attack shape', () => {
    nodeMinimatch(path, pattern)
  })
}

// --- filter 10k paths ---
{
  const rng = mulberry32(99)
  const paths = Array.from({ length: 10_000 }, () => randomPath(rng))
  bench('rs-minimatch: filter 10k', () => {
    rsMinimatch.match(paths, '**/*.ts')
  })
  bench('minimatch: filter 10k', () => {
    nodeMatch(paths, '**/*.ts')
  })
}

console.log('\n| Benchmark | rs-minimatch | minimatch | speedup |')
console.log('|---|---|---|---|')
for (let i = 0; i < results.length; i += 2) {
  const ours = results[i]
  const theirs = results[i + 1]
  const label = ours.name.replace('rs-minimatch: ', '')
  const speedup = (theirs.ms / ours.ms).toFixed(1)
  console.log(`| ${label} | ${ours.ms.toFixed(2)} ms | ${theirs.ms.toFixed(2)} ms | ${speedup}x |`)
}
