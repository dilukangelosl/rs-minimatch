//! Regression tests for real, published vulnerabilities in the reference
//! implementation this crate ports (brace-expansion's own security
//! advisories), not hypothetical ones. Each test names the real advisory.

use rs_minimatch_core::brace_expand;
use std::time::Instant;

/// CVE-2026-14257: `expand()` bounded the *count* of results but not their
/// *length*. `'{a,b}'.repeat(1500)` stays under any count cap while every
/// result grows one character per repeat, so total output size (and the
/// intermediate arrays combining it) grows unbounded and exhausts memory.
#[test]
fn cve_2026_14257_bounded_expansion_length() {
    let pattern = "{a,b}".repeat(1500);
    let start = Instant::now();
    let result = brace_expand(&pattern);
    let elapsed = start.elapsed();

    // The property that actually matters here is the memory bound below -
    // that's what turns "OOM crash" into "finishes, with a capped result".
    // This timing check just rules out catastrophic (unbounded/exponential)
    // blowup, not a performance target: naive Rust string concatenation
    // does real byte copies where V8's rope/cons-string representation
    // defers them, so matching the JS fix's ~0.7s isn't the bar here.
    assert!(elapsed.as_secs() < 10, "took {elapsed:?}, expected well under 10s");
    let total_len: usize = result.iter().map(|s| s.len()).sum();
    assert!(total_len <= rs_minimatch_core::MAX_LENGTH, "total output {total_len} exceeds MAX_LENGTH");
}

/// GHSA-3jxr-9vmj-r5cp: a run of non-expanding `{}` groups used to re-expand
/// the remaining tail once per group, doubling work each time. 30 groups (90
/// bytes) used to block for minutes.
#[test]
fn ghsa_3jxr_9vmj_r5cp_no_unbound_recursion() {
    let str = "a{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}";
    let start = Instant::now();
    let expanded = brace_expand(str);
    let elapsed = start.elapsed();

    assert_eq!(expanded, vec![str.to_string()], "non-expanding pattern should pass through unchanged");
    assert!(elapsed.as_millis() < 1000, "took {elapsed:?}, expected under 1s");
}

/// The original redos.js case: a huge run of commas inside a non-expanding
/// context shouldn't cause quadratic or worse blowup.
#[test]
fn redos_many_commas() {
    let str = format!("{{a}}{}\u{0}", ",".repeat(100_000));
    let start = Instant::now();
    let _ = brace_expand(&str);
    let elapsed = start.elapsed();
    assert!(elapsed.as_millis() < 1000, "took {elapsed:?}, expected under 1s");
}

/// General ReDoS-shape stress: deeply chained brace groups, not just the
/// two specific advisories above.
#[test]
fn deeply_chained_groups_stay_fast() {
    let pattern = "{a,b,c}".repeat(200);
    let start = Instant::now();
    let _ = brace_expand(&pattern);
    let elapsed = start.elapsed();
    assert!(elapsed.as_millis() < 2000, "took {elapsed:?}, expected well under 2s");
}
