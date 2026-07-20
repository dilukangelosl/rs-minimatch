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

use std::collections::HashMap;

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
    let mut memo = HashMap::new();
    match_at(pattern, file, 0, 0, opts, partial, &mut memo)
}

type Memo = HashMap<(usize, usize), bool>;

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
    if let Some(&hit) = memo.get(&(pi, fi)) {
        return hit;
    }
    let result = match_at_uncached(pattern, file, pi, fi, opts, partial, memo);
    memo.insert((pi, fi), result);
    result
}

fn match_at_uncached(pattern: &[Segment], file: &[String], pi: usize, fi: usize, opts: &Options, partial: bool, memo: &mut Memo) -> bool {
    if pi == pattern.len() && fi == file.len() {
        return true;
    }
    if fi == file.len() {
        // Ran out of file with pattern remaining. Fine in partial mode
        // (matching a path prefix against a longer pattern), or if
        // everything left in the pattern is globstars - `**` (and `**/**`)
        // can always collapse to zero segments, so e.g. plain "**" must
        // still match a single-segment file like "a".
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
            crate::matcher::segment_matches(nodes, seg, opts.nocase, dot_allowed) && match_at(pattern, file, pi + 1, fi + 1, opts, partial, memo)
        }
    }
}
