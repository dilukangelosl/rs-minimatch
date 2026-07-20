# rs-minimatch

**A drop-in, Rust-powered replacement for [`minimatch`](https://www.npmjs.com/package/minimatch)** — same API you already use (`minimatch()`, `.match()`, `filter`, `braceExpand`, the `Minimatch` class), backed by a hand-written matcher instead of compiled regex.

```sh
npm install rs-minimatch
```

```js
const minimatch = require('rs-minimatch')

minimatch('src/index.js', 'src/**/*.js') // true
minimatch.match(['a.js', 'a.ts', 'b.js'], '*.js') // ['a.js', 'b.js']
```

If you already use `minimatch` in a Node project, this is meant to be a
straight swap — same function names, same arguments, same behavior
(checked against `minimatch`'s own test suite — see [Project status](#project-status)
below).

## Why

`minimatch` compiles every glob pattern to a JavaScript regular
expression. Most patterns are fine, but a regex compiled from many
chained `**` groups can backtrack badly on certain inputs — the
well-known "ReDoS" risk category for glob/regex libraries. This crate
never compiles to regex at all: matching is memoized dynamic
programming over bounded index pairs, so there's no exponential case to
trigger in the first place, by construction rather than by a runtime
safety cap.

That said, this isn't a blanket performance upgrade — measured against
the real `minimatch` package on the same machine, same data:

| Operation | Speedup |
|---|---|
| Compile 1,000 patterns | **3.3x faster** |
| Match 10,000 paths | **1.4x faster** |
| Filter 10,000 paths | **27x slower** |

That last row is real, not a typo — see [Benchmarks](#benchmarks) below
for why, and what it'd take to close it.

## Also available as

- **A CLI**, for filtering paths by a glob pattern from the shell — see [CLI](#cli).
- **A Rust crate** (`rs-minimatch-core`), if you want the matcher without Node at all — see [Using it from Rust](#using-it-from-rust).

## More examples

```js
const minimatch = require('rs-minimatch')

// classes, same shape as the original minimatch package
const mm = new minimatch.Minimatch('src/**/*.{js,ts}')
mm.match('src/a/b.ts') // true
mm.pattern // 'src/**/*.{js,ts}'

// extglobs, character classes, brace expansion - all supported
minimatch('foo.js', '+(foo|bar).js') // true
minimatch('.foo', '*', { dot: true }) // true
minimatch.braceExpand('a{b,c}d') // ['abd', 'acd']

// import just what you need, same subpaths as `minimatch`
const filter = require('rs-minimatch/functions/filter')
;['a.js', 'a.ts'].filter(filter('*.js')) // ['a.js']
```

## CLI

```sh
cargo build --release -p rs-minimatch-cli
./target/release/rs-minimatch '*.js' foo.js bar.ts baz.js
# foo.js
# baz.js
```

Reads paths from stdin if none are given as arguments, so it composes
with `find`/`ls`/etc. Run `rs-minimatch --help` for the full flag list.

## Using it from Rust

```rust
use rs_minimatch_core::{minimatch, Options};

assert!(minimatch("src/index.js", "src/**/*.js", Options::default()));
```

`rs-minimatch-core` is a standalone, zero-dependency crate — it doesn't
need Node.js at all. See `crates/core/src/lib.rs` for the full function
list; it mirrors the npm package's API (`minimatch`, `match_list`,
`filter`, `brace_expand`, `escape`/`unescape`), plus the `Minimatch`
type.

## Project status

This was built from scratch and checked against
[minimatch's own test suite](crates/core/tests/minimatch_compat.rs)
(the real `test/patterns.js` fixtures from v10.2.5, replayed through the
actual npm package rather than hand-picked cases) — roughly 180 of 196
cases pass. The rest are documented gaps, not silent holes:

- `..` path-segment resolution during matching (`x/*/../a/b/c`-style
  patterns) isn't implemented — a genuinely separate feature from glob
  matching itself.
- An extglob with a completely empty body (`+()`) falls back to literal
  matching in real minimatch; not implemented here.
- A handful of deep escaping and adjacent-extglob edge cases.

Two real, published vulnerabilities in the packages this crate is based
on are covered by dedicated regression tests, not just mentioned:
[CVE-2026-14257](crates/core/tests/brace_security.rs) (an unbounded
expansion-length DoS in `brace-expansion`, confirmed directly from that
package's own advisory) and GHSA-3jxr-9vmj-r5cp (unbounded recursion on
chained empty brace groups).

`makeRe()` / `.regexp` aren't implemented — this crate never compiles
patterns to regex internally, and generating an equivalent regex just
for that one compatibility property is out of scope for now.

<details>
<summary><strong>Contributor / internals details</strong> (repo layout, building from source, benchmarks, CI/CD, design notes)</summary>

## Repo layout

```
crates/
  core/    rs-minimatch-core — the matcher, zero dependencies
  cli/     rs-minimatch-cli  — CLI binary
  napi/    rs-minimatch-napi — NAPI-RS bindings, thin wrapper over core
packages/
  rs-minimatch/  npm package: functions/ (per-symbol require paths,
                 matching minimatch's layout) + index.js (full API surface)
```

## Building from source

```sh
# Rust library + CLI
cargo build --release
cargo test
cargo bench -p rs-minimatch-core

# Node bindings
cd packages/rs-minimatch
npm run build   # cargo builds the addon, copies it to rs-minimatch.node
node -e "console.log(require('.')('a.js', '*.js'))"
```

## CI/CD

- **`.github/workflows/ci.yml`** — runs on every push/PR to `main`:
  `cargo build`/`test`/`clippy` across the workspace, plus a Node smoke
  test that builds the addon in debug mode and exercises it.
- **`.github/workflows/release.yml`** — triggered by pushing a version
  tag. Builds the NAPI addon for macOS (arm64), Linux (x64 + arm64,
  glibc), and Windows (x64), publishes each as its own
  `rs-minimatch-<platform>-<arch>` npm package, then publishes the main
  `rs-minimatch` package with those as `optionalDependencies` — the
  same prebuilt-binary pattern `sharp`/`esbuild`/`@swc/core` use, and
  the same platform set rs-semver's release workflow settled on
  (darwin-x64 dropped: GitHub's Intel Mac runner pool sat queued 25+
  minutes with nothing assigned on that repo's first real release run).
  Needs an `NPM_TOKEN` repo secret before it'll actually publish.
- Only builds glibc Linux binaries; musl (Alpine) isn't covered.
- crates.io publishing isn't wired up.

## Benchmarks

Measured on a single Apple Silicon (arm64) dev machine, one run each,
`cargo build --release`. Not a controlled, multi-machine, statistically
rigorous benchmark suite — treat these as directional, and reproduce them
yourself before relying on them (commands below).

**Pure Rust core** (`cargo bench -p rs-minimatch-core`, criterion, 100
samples each):

| Benchmark | Median time |
|---|---|
| Match 10,000 random paths against a pattern | 20.5 ms |
| Compile 1,000 patterns | 1.8 ms |
| Match against 11 chained globstars over 30 path segments | 11.8 µs |
| Filter 10,000 paths | 65.7 ms |

**Node.js: `rs-minimatch` (via NAPI) vs. the real `minimatch` package**
(`benchmarks/compare.js`, same generated data for both, best-of-5,
identical process):

| Benchmark | rs-minimatch | minimatch | speedup |
|---|---|---|---|
| Match 10,000 paths | 22.36 ms | 31.04 ms | **1.4x** |
| Compile 1,000 patterns | 2.22 ms | 7.22 ms | **3.3x** |
| 11 chained globstars / 30 segments (the PRD's ReDoS example) | 0.01 ms | 0.00 ms | **0.3x** |
| Filter 10,000 paths | 65.50 ms | 2.44 ms | **0.04x (27x slower)** |

Two things worth being direct about instead of only reporting the
numbers that look good:

**The ReDoS attack shape isn't actually slow on real minimatch anymore.**
The PRD this project is based on describes 11 chained `**` groups over
30 path segments as a 5+ second stall. On the actual current package
(v10.2.5) it runs in about 1.3ms — they already ship a
`maxGlobstarRecursion` cap as a mitigation. This crate gets the same
safety property a different way: the memoized-DP matcher has no
exponential case to begin with, so there's nothing to cap. That matters
because a depth cap can produce false negatives on legitimately deep
(non-adversarial) patterns, while an algorithm with no unbounded case
can't — but "we fixed a live vulnerability" would be a false claim, and
isn't the one being made here.

**Filtering 10,000 paths is 27x slower, and it's a real, traced cause,
not FFI overhead.** The pure-Rust criterion benchmark above (no FFI
involved at all) shows the identical ~65ms, so the NAPI boundary isn't
the story here. The actual cause: every call into the matcher allocates
a fresh `HashMap` for its memoization table, even for a plain segment
(a bare `*` or a literal) that doesn't need dynamic programming at all —
a simple linear scan would do. `minimatch` compiles the whole pattern to
one regex once and lets a heavily-optimized native regex engine test
each path in a single call; this crate re-walks its own DP machinery
per path with real per-call allocation cost. The worst-case safety
property holds regardless; everyday-pattern throughput currently does
not win, and that's worth knowing before reaching for this crate to
speed up a hot filtering path. The concrete next step, if this gets
pushed further, is a fast path that skips the memo table entirely for
segments with no extglob nodes — not a rewrite of the safety model.

Reproduce:

```sh
cargo bench -p rs-minimatch-core

cd packages/rs-minimatch && npm run build && cd ../../benchmarks
npm install
node compare.js
```

## Design notes

- **No regex, no backtracking parser.** Matching is memoized dynamic
  programming over `(pattern-node-index, text-position)` pairs — the
  same technique that makes regex engines like Rust's own `regex` crate
  immune to catastrophic backtracking, applied directly instead of
  through a compiled automaton.
- **Full extglob support** (`!(...)`, `?(...)`, `+(...)`, `*(...)`,
  `@(...)`, including nesting), implemented as the same DP generalized
  to try each alternative against each candidate span.
- **The leading-dot exclusion rule** (`*` doesn't match `.foo` unless
  `dot` is set, but `.*` and a handful of extglob forms do) was worked
  out empirically against the real package rather than guessed from its
  regex-generation source — see the comments in `matcher.rs` for exactly
  which extglob types disable the guard and why.
- **Brace expansion is a direct port** of the `brace-expansion` npm
  package's own algorithm (balanced-match included), bounded on both
  result count and total output length — the second bound is what
  turns a real, published DoS (CVE-2026-14257) into a capped, non-fatal
  result instead of a crash.

</details>

## License

MIT
