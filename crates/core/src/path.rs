//! Whole-path matching: splits a pattern/file into `/`-separated segments
//! and matches segment-by-segment, with `**` (globstar) handled by the same
//! memoized-DP approach as everything else in this crate. This is the part
//! that's actually exposed to the CVE-2026-27903-style attack the PRD is
//! about (many chained `**` groups): memoizing on (pattern-segment-index,
//! file-segment-index) caps total work at O(P x F) no matter how many `**`
//! occur, so there's no exponential case to trigger in the first place -
//! this crate doesn't need minimatch's own `maxGlobstarRecursion` depth cap
//! (a heuristic mitigation) because the algorithm has no unbounded case to
//! cap.

use crate::options::Options;
use crate::pattern::{self, Node};

#[derive(Debug, Clone)]
pub enum Segment {
    GlobStar,
    Pattern(Vec<Node>),
}

pub fn split_path(s: &str, preserve_multiple_slashes: bool) -> Vec<String> {
    if preserve_multiple_slashes {
        s.split('/').map(str::to_string).collect()
    } else {
        // collapse runs of '/' into one split point
        let mut parts = Vec::new();
        let mut cur = String::new();
        let mut chars = s.chars().peekable();
        if s.is_empty() {
            return vec![String::new()];
        }
        while let Some(c) = chars.next() {
            if c == '/' {
                parts.push(std::mem::take(&mut cur));
                while chars.peek() == Some(&'/') {
                    chars.next();
                }
            } else {
                cur.push(c);
            }
        }
        parts.push(cur);
        parts
    }
}

pub fn compile_segments(pattern: &str, opts: &Options) -> Vec<Segment> {
    let parts = split_path(pattern, opts.preserve_multiple_slashes);
    parts
        .into_iter()
        .map(|p| {
            if p == "**" && !opts.noglobstar {
                Segment::GlobStar
            } else if p == "**" {
                Segment::Pattern(pattern::parse_segment("*", opts.noext))
            } else {
                Segment::Pattern(pattern::parse_segment(&p, opts.noext))
            }
        })
        .collect()
}

pub fn basename(file_segments: &[String]) -> &str {
    if let Some(last) = file_segments.last() {
        if !last.is_empty() {
            return last;
        }
    }
    for seg in file_segments.iter().rev() {
        if !seg.is_empty() {
            return seg;
        }
    }
    ""
}

pub fn match_segments(pattern: &[Segment], file: &[String], opts: &Options, partial: bool) -> bool {
    // The extremely common "**/fixed/tail" shape (`**/*.ts`, `**/foo/bar.js`
    // - a leading globstar with no further globstar after it) has only one
    // or two possible alignments at all (see `match_leading_globstar_fixed_tail`),
    // so it's cheaper to compute those directly than to let the general
    // recursive algorithm rediscover the same thing by trying every
    // candidate globstar split. Every other shape (globstar in the middle,
    // more than one globstar, no globstar at all) is untouched.
    if !partial {
        if let [Segment::GlobStar, tail @ ..] = pattern {
            if !tail.is_empty() && !tail.iter().any(|s| matches!(s, Segment::GlobStar)) {
                return match_leading_globstar_fixed_tail(tail, file, opts);
            }
        }
    }
    let mut memo = Memo::for_shape(pattern, pattern.len(), file.len());
    match_at(pattern, file, 0, 0, opts, partial, &mut memo)
}

/// Fast path for `[GlobStar, tail...]` where `tail` has no globstar of its
/// own: since `tail` has no globstar of its own, it must consume exactly
/// `tail.len()` file segments ending at the very end of the path (or one
/// short of the end, to allow for a single trailing slash) - there's only
/// one or two possible alignments to check, never a search over many
/// candidate globstar splits. Mirrors real minimatch's own head/tail
/// decomposition for this exact shape (`#matchGlobstar`'s
/// `tailStart`/`tailStart - 1`), including its own comment: "affordance for
/// stuff like `a/**/*` matching `a/b/`" - a single trailing slash shifts
/// the whole tail one segment earlier, which is the only other alignment
/// that could ever legitimately succeed.
fn match_leading_globstar_fixed_tail(tail: &[Segment], file: &[String], opts: &Options) -> bool {
    let tail_len = tail.len();
    if file.len() < tail_len {
        return false;
    }

    let try_alignment = |tail_start: usize| -> bool {
        for (i, seg_pattern) in tail.iter().enumerate() {
            let fi = tail_start + i;
            let seg = &file[fi];
            let Segment::Pattern(nodes) = seg_pattern else {
                unreachable!("tail is guaranteed globstar-free by the caller")
            };
            if traversal_blocked(seg, pattern::is_only_dots(nodes)) {
                return false;
            }
            // Same rule as the general algorithm: a real pattern segment
            // never consumes the empty artifact of a trailing slash itself
            // - only the fallback alignment below (shifting the whole tail
            // one segment earlier) is allowed to leave it dangling off the
            // end unconsumed.
            if !nodes.is_empty() && seg.is_empty() && fi == file.len() - 1 {
                return false;
            }
            let dot_allowed = opts.dot || pattern::starts_with_literal_dot(nodes);
            if !crate::matcher::segment_matches(nodes, seg, opts.nocase, dot_allowed) {
                return false;
            }
        }
        // Everything before the tail is free-floating "**" territory: any
        // segments are fine except "." / ".." / hidden-without-dot, same
        // rule the GlobStar arm's own consume step applies.
        file[..tail_start].iter().all(|seg| seg != "." && seg != ".." && (opts.dot || !seg.starts_with('.')))
    };

    let primary_tail_start = file.len() - tail_len;
    if try_alignment(primary_tail_start) {
        return true;
    }
    if primary_tail_start >= 1 && file.last().is_some_and(|s| s.is_empty()) {
        return try_alignment(primary_tail_start - 1);
    }
    false
}

/// Test-only: always takes the general recursive route, bypassing the
/// `match_leading_globstar_fixed_tail` shortcut entirely regardless of
/// shape. Used to prove that shortcut never *disagrees* with the general
/// algorithm it's replacing - the real regression bar for a shortcut like
/// this, independent of whether either one happens to be correct per real
/// minimatch on any given case.
#[cfg(test)]
pub(crate) fn match_segments_general_only(pattern: &[Segment], file: &[String], opts: &Options, partial: bool) -> bool {
    let mut memo = Memo::for_shape(pattern, pattern.len(), file.len());
    match_at(pattern, file, 0, 0, opts, partial, &mut memo)
}

/// A single `**` can't blow up without memoization either (see the same
/// argument in `matcher::needs_memo_table`) - it's *chained* globstars
/// re-trying the same file suffix from different pattern positions that
/// need it. Most real patterns (`*.ts`, `src/*.js`, or even one `**`) never
/// hit that case at all, so they skip the table entirely.
fn needs_memo_table(pattern: &[Segment]) -> bool {
    pattern.iter().filter(|s| matches!(s, Segment::GlobStar)).count() > 1
}

/// Memoization table keyed on (pattern segment index, file segment index).
/// `Table` is a flat `Vec` sized once up front instead of a `HashMap` - same
/// recursive algorithm and the same polynomial-time guarantee, just without
/// per-lookup hashing and incremental-growth reallocation. `Skip` allocates
/// nothing at all, for the shapes `needs_memo_table` has proven don't need
/// caching. This gets built fresh on every `match_segments` call, so this
/// overhead was showing up directly in bulk filtering throughput.
enum Memo {
    Skip,
    Table { cells: Vec<Option<bool>>, cols: usize },
}

impl Memo {
    fn for_shape(pattern: &[Segment], rows: usize, cols: usize) -> Self {
        if needs_memo_table(pattern) {
            let cols = cols + 1;
            Memo::Table { cells: vec![None; (rows + 1) * cols], cols }
        } else {
            Memo::Skip
        }
    }

    fn get(&self, pi: usize, fi: usize) -> Option<bool> {
        match self {
            Memo::Skip => None,
            Memo::Table { cells, cols } => cells[pi * cols + fi],
        }
    }

    fn set(&mut self, pi: usize, fi: usize, value: bool) {
        if let Memo::Table { cells, cols } = self {
            cells[pi * *cols + fi] = Some(value);
        }
    }
}

/// `.` and `..` are never matched by magic, even under `dot: true` - only a
/// pattern that's the literal string "." or ".." matches them. The general
/// "wildcards don't eat a leading dot unless `dot` is set" rule lives in
/// `matcher::segment_matches` instead, since it depends on which node
/// within the pattern actually consumes that character, not just whether
/// the file segment happens to start with one.
fn traversal_blocked(seg: &str, pattern_is_only_dots: bool) -> bool {
    (seg == "." || seg == "..") && !pattern_is_only_dots
}

fn match_at(pattern: &[Segment], file: &[String], pi: usize, fi: usize, opts: &Options, partial: bool, memo: &mut Memo) -> bool {
    if let Some(hit) = memo.get(pi, fi) {
        return hit;
    }
    let result = match_at_uncached(pattern, file, pi, fi, opts, partial, memo);
    memo.set(pi, fi, result);
    result
}

/// Does a bare trailing `**` (nothing after it in the pattern) match
/// `file[entry_fi..]`? Real minimatch requires it to sweep at least one
/// segment - `a/**` matches `a/` and `a/b` but not `a` itself, because the
/// pattern spells out a literal `/` that a bare `a` never contains. This is
/// computed directly (not through the memoized `(pi, fi)` table) precisely
/// *because* it must never be shared between two different call sites: a
/// literal segment handing off into a fresh globstar run (nothing consumed
/// yet - must fail here) and that same globstar mid-recursion after it has
/// already eaten something (must succeed) can otherwise land on the exact
/// same `(pi, fi)` cell with opposite correct answers. See the Pattern-arm
/// caller below.
fn sweep_trailing_globstar(file: &[String], entry_fi: usize, dot: bool) -> bool {
    if entry_fi >= file.len() {
        return false;
    }
    file[entry_fi..]
        .iter()
        .all(|seg| seg != "." && seg != ".." && (dot || !seg.starts_with('.')))
}

fn match_at_uncached(pattern: &[Segment], file: &[String], pi: usize, fi: usize, opts: &Options, partial: bool, memo: &mut Memo) -> bool {
    if pi == pattern.len() && fi == file.len() {
        return true;
    }
    if fi == file.len() {
        // Ran out of file with pattern remaining. Fine in partial mode
        // (matching a path prefix against a longer pattern). Otherwise only
        // true if everything left is globstars *and* we got here through
        // one of them actually consuming something (self-recursion within
        // this same run, e.g. plain "**" eating a whole single-segment
        // file) - a *fresh* zero-consumption entry into a trailing run is
        // handled and rejected by the Pattern arm below before it ever
        // reaches this cell, so reaching here with a pure-globstar suffix
        // only happens via legitimate in-run consumption.
        return partial || pattern[pi..].iter().all(|s| matches!(s, Segment::GlobStar));
    }
    if pi == pattern.len() {
        // Ran out of pattern with file left: only OK for one trailing empty
        // segment (a path ending in '/'), e.g. "a/*" matching "a/b/".
        return fi == file.len() - 1 && file[fi].is_empty();
    }

    match &pattern[pi] {
        Segment::GlobStar => {
            if match_at(pattern, file, pi + 1, fi, opts, partial, memo) {
                return true;
            }
            let seg = &file[fi];
            if seg == "." || seg == ".." || (!opts.dot && seg.starts_with('.')) {
                // "." and ".." (and dotfiles without `dot`) never fall
                // inside a globstar's consumed range.
                return false;
            }
            match_at(pattern, file, pi, fi + 1, opts, partial, memo)
        }
        Segment::Pattern(nodes) => {
            let seg = &file[fi];
            if traversal_blocked(seg, pattern::is_only_dots(nodes)) {
                return false;
            }
            // A trailing slash produces one empty final file segment (`a/b/`
            // -> ["a","b",""]). A real pattern segment (e.g. `*`) must never
            // consume that artifact itself - only the "pattern exhausted,
            // one trailing empty segment left" rule below is allowed to
            // absorb it, which requires this segment to have matched a real
            // preceding one first. `a/*` matches `a/b/` (star matches "b",
            // then that rule covers the trailing ""), but not `a/` (star
            // would have to consume the "" directly).
            if !nodes.is_empty() && seg.is_empty() && fi == file.len() - 1 {
                return false;
            }
            let dot_allowed = opts.dot || pattern::starts_with_literal_dot(nodes);
            if !crate::matcher::segment_matches(nodes, seg, opts.nocase, dot_allowed) {
                return false;
            }
            let (next_pi, next_fi) = (pi + 1, fi + 1);
            if next_pi < pattern.len() && pattern[next_pi..].iter().all(|s| matches!(s, Segment::GlobStar)) {
                return partial || sweep_trailing_globstar(file, next_fi, opts.dot);
            }
            match_at(pattern, file, next_pi, next_fi, opts, partial, memo)
        }
    }
}

#[cfg(test)]
mod leading_globstar_fixed_tail_differential_tests {
    use super::{compile_segments, match_segments, match_segments_general_only, split_path};
    use crate::options::Options;

    struct Rng(u32);
    impl Rng {
        fn next_u32(&mut self) -> u32 {
            self.0 = self.0.wrapping_add(0x6d2b79f5);
            let mut t = self.0;
            t = (t ^ (t >> 15)).wrapping_mul(t | 1);
            t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
            t ^ (t >> 14)
        }
        fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
            &items[(self.next_u32() as usize) % items.len()]
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next_u32() as usize) % n
        }
    }

    fn random_path(rng: &mut Rng) -> String {
        let segs = ["a", "b", ".foo", "c.ts", "d.js", ".."];
        let n = 1 + rng.below(4);
        let mut parts: Vec<&str> = (0..n).map(|_| *rng.pick(&segs)).collect();
        if rng.below(4) == 0 {
            parts.push(""); // trailing slash
        }
        parts.join("/")
    }

    fn random_globstar_tail_pattern(rng: &mut Rng) -> String {
        let tail_pieces = ["*.ts", "*", "?.js", "[ab]*", "a", ".foo", "**"];
        // Bias heavily toward exactly one leading globstar with a
        // globstar-free tail - the shape the fast path actually targets -
        // but occasionally include a second "**" so some generated
        // patterns fall through to the general path too, as a sanity
        // check that both routes still get exercised.
        let n_tail = 1 + rng.below(3);
        let mut parts = vec!["**".to_string()];
        for _ in 0..n_tail {
            parts.push((*rng.pick(&tail_pieces)).to_string());
        }
        parts.join("/")
    }

    #[test]
    fn fast_path_never_disagrees_with_general_algorithm() {
        let mut rng = Rng(0xFEED_FACE);
        let mut checked = 0;
        for _ in 0..20_000 {
            let path = random_path(&mut rng);
            let pattern_str = random_globstar_tail_pattern(&mut rng);
            for nocase in [false, true] {
                for dot in [false, true] {
                    let opts = Options { nocase, dot, ..Options::default() };
                    let pattern = compile_segments(&pattern_str, &opts);
                    let file = split_path(&path, opts.preserve_multiple_slashes);
                    for partial in [false, true] {
                        let fast = match_segments(&pattern, &file, &opts, partial);
                        let general = match_segments_general_only(&pattern, &file, &opts, partial);
                        assert_eq!(
                            fast, general,
                            "disagreement on path={path:?} pattern={pattern_str:?} nocase={nocase} dot={dot} partial={partial}: fast={fast} general={general}"
                        );
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 0);
    }
}
