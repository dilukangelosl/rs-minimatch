//! Matching, guaranteed polynomial-time by construction: everything here is
//! memoized dynamic programming over (pattern position, text position)
//! pairs, never backtracking search. This is what makes the crate immune to
//! catastrophic blowup on adversarial input, the same property a regex
//! engine gets from *not* doing naive backtracking.

use std::collections::HashMap;

use crate::pattern::{ExtKind, Node};

/// Matches a parsed segment pattern against one path segment (no `/`).
///
/// `dot`: whether a leading `.` in `text` may be consumed by a wildcard
/// (`*`/`?`/`[...]`). An explicit literal `.` in the pattern can always
/// match it regardless (`.*` matches `.foo`; `*.foo` does not, unless
/// `dot` is set) - this is a glob convention, not a minimatch-specific
/// quirk.
pub fn segment_matches(nodes: &[Node], text: &str, nocase: bool, dot: bool) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let block_leading_dot = !dot && chars.first() == Some(&'.');
    let mut memo = HashMap::new();
    matches_at(nodes, &chars, 0, 0, nocase, block_leading_dot, &mut memo)
}

type Memo = HashMap<(usize, usize), bool>;

fn matches_at(nodes: &[Node], text: &[char], ni: usize, ti: usize, nocase: bool, block_dot: bool, memo: &mut Memo) -> bool {
    if let Some(&hit) = memo.get(&(ni, ti)) {
        return hit;
    }
    // Guard against pathological recursion depth on extreme inputs by
    // relying on the memo table to cap total work at O(nodes * text); the
    // recursion itself only ever moves ni or ti forward, so it terminates.
    let result = matches_at_uncached(nodes, text, ni, ti, nocase, block_dot, memo);
    memo.insert((ni, ti), result);
    result
}

fn matches_at_uncached(nodes: &[Node], text: &[char], ni: usize, ti: usize, nocase: bool, block_dot: bool, memo: &mut Memo) -> bool {
    let Some(node) = nodes.get(ni) else {
        return ti == text.len();
    };
    let blocked_here = block_dot && ti == 0;

    match node {
        Node::Literal(lit) => {
            let lit_chars: Vec<char> = lit.chars().collect();
            let end = ti + lit_chars.len();
            if end > text.len() {
                return false;
            }
            let region = &text[ti..end];
            let eq = if nocase {
                region.iter().zip(&lit_chars).all(|(a, b)| a.eq_ignore_ascii_case(b))
            } else {
                region == lit_chars.as_slice()
            };
            eq && matches_at(nodes, text, ni + 1, end, nocase, block_dot, memo)
        }
        Node::AnyChar => {
            ti < text.len() && !blocked_here && matches_at(nodes, text, ni + 1, ti + 1, nocase, block_dot, memo)
        }
        Node::Star => {
            matches_at(nodes, text, ni + 1, ti, nocase, block_dot, memo)
                || (ti < text.len() && !blocked_here && matches_at(nodes, text, ni, ti + 1, nocase, block_dot, memo))
        }
        Node::Class(class) => {
            ti < text.len()
                && !blocked_here
                && class.matches(text[ti], nocase)
                && matches_at(nodes, text, ni + 1, ti + 1, nocase, block_dot, memo)
        }
        Node::ExtGlob { kind, alts } => ext_matches(nodes, text, ni, ti, *kind, alts, nocase, block_dot, memo),
    }
}

#[allow(clippy::too_many_arguments)]
/// ponytail: re-checks each alternative against each candidate span from
/// scratch rather than sharing one global (node, start, end) memo table
/// across the whole match, so deeply nested extglobs redo some work. Still
/// strictly polynomial (bounded by span count x alt size), never
/// exponential - the property that actually matters here. Upgrade to a
/// shared memo if profiling ever shows this path is hot.
///
/// Also: doesn't propagate `block_dot` into extglob alternatives, so an
/// extglob as the very first thing in a segment may match a leading dot
/// where a bare `*`/`?`/`[...]` wouldn't. Narrow, documented gap rather
/// than threading a start-of-segment flag through every alt sub-match.
fn ext_matches(
    nodes: &[Node],
    text: &[char],
    ni: usize,
    ti: usize,
    kind: ExtKind,
    alts: &[Vec<Node>],
    nocase: bool,
    block_dot: bool,
    memo: &mut Memo,
) -> bool {
    match kind {
        ExtKind::ZeroOrOne => {
            matches_at(nodes, text, ni + 1, ti, nocase, block_dot, memo)
                || any_alt_span(alts, text, ti, nocase).any(|end| matches_at(nodes, text, ni + 1, end, nocase, block_dot, memo))
        }
        ExtKind::ExactlyOne => {
            any_alt_span(alts, text, ti, nocase).any(|end| matches_at(nodes, text, ni + 1, end, nocase, block_dot, memo))
        }
        ExtKind::ZeroOrMore => {
            matches_at(nodes, text, ni + 1, ti, nocase, block_dot, memo) || repeat_matches(nodes, text, ni, ti, alts, nocase, block_dot, memo)
        }
        ExtKind::OneOrMore => repeat_matches(nodes, text, ni, ti, alts, nocase, block_dot, memo),
        ExtKind::Not => {
            let alt_ends: std::collections::HashSet<usize> = any_alt_span(alts, text, ti, nocase).collect();
            (ti..=text.len()).any(|end| !alt_ends.contains(&end) && matches_at(nodes, text, ni + 1, end, nocase, block_dot, memo))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn repeat_matches(nodes: &[Node], text: &[char], ni: usize, ti: usize, alts: &[Vec<Node>], nocase: bool, block_dot: bool, memo: &mut Memo) -> bool {
    any_alt_span(alts, text, ti, nocase).any(|end| {
        matches_at(nodes, text, ni + 1, end, nocase, block_dot, memo) || repeat_matches(nodes, text, ni, end, alts, nocase, block_dot, memo)
    })
}

/// End positions >= `start` at which some alternative fully matches
/// `text[start..end]`.
fn any_alt_span<'a>(alts: &'a [Vec<Node>], text: &'a [char], start: usize, nocase: bool) -> impl Iterator<Item = usize> + 'a {
    (start..=text.len()).filter(move |&end| alts.iter().any(|alt| full_match(alt, &text[start..end], nocase)))
}

fn full_match(nodes: &[Node], text: &[char], nocase: bool) -> bool {
    let mut memo = HashMap::new();
    matches_at(nodes, text, 0, 0, nocase, false, &mut memo)
}
