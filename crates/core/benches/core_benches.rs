//! Benchmarks matching the PRD's methodology: matching 10k paths against a
//! pattern, compiling 1k patterns, matching against a chained-globstar
//! pattern (the attack shape the PRD's ReDoS section is about), and
//! filtering 10k paths. Run with `cargo bench -p rs-minimatch-core`.
//!
//! These measure this crate in isolation - they are not a comparison
//! against minimatch (that would need a JS harness alongside this one, see
//! benchmarks/compare.js at the repo root) and no throughput numbers
//! should be taken from this file alone as a finished perf claim.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rs_minimatch_core::{match_list, minimatch, Options};

fn random_path(rng: &mut StdRng) -> String {
    let dirs = ["src", "lib", "test", "node_modules", "dist", "components"];
    let exts = ["js", "ts", "json", "css", "md"];
    let depth = rng.gen_range(1..=4);
    let mut parts = Vec::new();
    for _ in 0..depth {
        parts.push(dirs[rng.gen_range(0..dirs.len())].to_string());
    }
    let ext = exts[rng.gen_range(0..exts.len())];
    parts.push(format!("file{}.{ext}", rng.gen_range(0..1000)));
    parts.join("/")
}

fn bench_match(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(42);
    let paths: Vec<String> = (0..10_000).map(|_| random_path(&mut rng)).collect();
    let opts = Options::default();

    c.bench_function("match_10k_paths", |b| {
        b.iter(|| {
            for p in &paths {
                black_box(minimatch(black_box(p), black_box("src/**/*.js"), opts.clone()));
            }
        })
    });
}

fn bench_compile(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(7);
    let patterns: Vec<String> = (0..1_000)
        .map(|_| {
            let dirs = ["src", "lib", "test"];
            format!("{}/**/*.{{js,ts}}", dirs[rng.gen_range(0..dirs.len())])
        })
        .collect();

    c.bench_function("compile_1k_patterns", |b| {
        b.iter(|| {
            for p in &patterns {
                black_box(rs_minimatch_core::Minimatch::new(black_box(p), Options::default()));
            }
        })
    });
}

fn bench_globstar_attack_shape(c: &mut Criterion) {
    // Same shape as the PRD's own CVE-2026-27903 example: k chained
    // globstars over n path segments.
    let pattern = "**/".repeat(11) + "foo";
    let path = "a/".repeat(30) + "foo";
    let opts = Options::default();

    c.bench_function("globstar_11x_over_30_segments", |b| {
        b.iter(|| black_box(minimatch(black_box(&path), black_box(&pattern), opts.clone())))
    });
}

fn bench_filter(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(99);
    let paths: Vec<String> = (0..10_000).map(|_| random_path(&mut rng)).collect();
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let opts = Options::default();

    c.bench_function("filter_10k_paths", |b| {
        b.iter(|| black_box(match_list(black_box(&refs), black_box("**/*.ts"), opts.clone())))
    });
}

criterion_group!(benches, bench_match, bench_compile, bench_globstar_attack_shape, bench_filter);
criterion_main!(benches);
