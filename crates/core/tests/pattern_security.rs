//! Regression tests for genuine catastrophic-blowup shapes in this crate's
//! own matcher, found by direct adversarial testing rather than assumed
//! away. The whole pitch of a memoized-DP matcher is that these can't
//! happen - each test here is a case where that pitch was checked, not just
//! asserted.

use rs_minimatch_core::{minimatch, Options};
use std::time::Instant;

/// `+(x)`/`*(x)` repetition recurses on itself directly rather than through
/// the shared `matches_at` memo, so without its own cache it's the textbook
/// unmemoized word-break blowup: `+(a|aa)` against a long run of `a`s can
/// reach the same (extglob node, text position) state through many
/// different ways of splitting the run into 1s and 2s, and every one of
/// them re-explores the rest of the string from scratch. This was a real,
/// measured ~5.2 second stall at 35 characters before the repetition memo
/// was added - not a hypothetical.
#[test]
fn quantified_extglob_alternation_stays_polynomial() {
    let pattern = "+(a|aa)x";
    for n in [100, 200, 400, 800] {
        let text = "a".repeat(n);
        let start = Instant::now();
        let matched = minimatch(&text, pattern, Options::default());
        let elapsed = start.elapsed();
        assert!(!matched, "text of {n} a's should never match, it never contains an x");
        assert!(elapsed.as_millis() < 500, "n={n} took {elapsed:?}, expected well under 500ms");
    }
}

/// Same shape, `*(x)` (zero-or-more) instead of `+(x)` (one-or-more).
#[test]
fn quantified_zero_or_more_extglob_stays_polynomial() {
    let pattern = "*(a|aa)x";
    let text = "a".repeat(500);
    let start = Instant::now();
    let matched = minimatch(&text, pattern, Options::default());
    let elapsed = start.elapsed();
    assert!(!matched);
    assert!(elapsed.as_millis() < 500, "took {elapsed:?}, expected well under 500ms");
}

/// Many chained `*` in a single segment - the classic wildcard-matching
/// ReDoS shape (`a?*?*?*?*?*?*` and friends), not extglob-specific.
#[test]
fn many_chained_stars_stay_fast() {
    let pattern = format!("{}x", "*".repeat(50));
    let text = "a".repeat(200);
    let start = Instant::now();
    let matched = minimatch(&text, &pattern, Options::default());
    let elapsed = start.elapsed();
    assert!(!matched);
    assert!(elapsed.as_millis() < 500, "took {elapsed:?}, expected well under 500ms");
}
