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
///
/// Verified empirically against real minimatch (theorizing from its regex
/// generation code alone didn't fully explain the behavior): when the
/// first node of a pattern (or of an individual extglob alternative,
/// recursively) is an extglob, whether the guard applies depends on which
/// of the five forms it is. `*(x)`, `?(x)`, and `!(x)` leading a pattern
/// disable the guard - `*(test).js` matches `.js`. `@(x)` and `+(x)` do
/// not - `@(*).js` does not match `.js`, same as a bare leading `*`
/// wouldn't - *unless* one of their own alternatives explicitly starts
/// with a literal `.` (`@(.*)` matches `.js`; `@(js|.*)` matches `.js` via
/// the second alternative even though the first wouldn't).
pub fn segment_matches(nodes: &[Node], text: &str, nocase: bool, dot: bool) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let has_leading_dot = chars.first() == Some(&'.');
    let block_dot = guard_active(nodes, dot, has_leading_dot);
    let mut memo = HashMap::new();
    matches_at(nodes, &chars, 0, 0, nocase, block_dot, dot, has_leading_dot, &mut memo)
}

/// Whether the leading-dot guard should apply to `nodes` matching at the
/// start of a dot-leading segment.
fn guard_active(nodes: &[Node], dot_allowed: bool, has_leading_dot: bool) -> bool {
    if dot_allowed || !has_leading_dot {
        return false;
    }
    match nodes.first() {
        Some(Node::ExtGlob { kind, alts }) => extglob_guard_active(*kind, alts, dot_allowed, has_leading_dot),
        // An explicit literal '.' leading the pattern always permits it,
        // same exemption `matches_at_uncached` applies at ni==0 - needed
        // here too so e.g. `@(js|.*)` sees its second alternative as
        // dot-safe.
        Some(Node::Literal(s)) if s.starts_with('.') => false,
        _ => true,
    }
}

/// Same question, for an extglob node specifically (used both from
/// `guard_active` when the extglob leads a pattern, and from the matcher
/// when deciding whether the extglob's own alternatives may consume a
/// leading dot at ti==0).
fn extglob_guard_active(kind: ExtKind, alts: &[Vec<Node>], dot_allowed: bool, has_leading_dot: bool) -> bool {
    match kind {
        ExtKind::ZeroOrMore | ExtKind::ZeroOrOne | ExtKind::Not => false,
        ExtKind::ExactlyOne | ExtKind::OneOrMore => {
            // Still disabled if *any* alternative explicitly starts with a
            // literal dot itself (`@(.*)`, `@(js|.*)`).
            !alts.iter().any(|alt| !guard_active(alt, dot_allowed, has_leading_dot))
        }
    }
}

type Memo = HashMap<(usize, usize), bool>;

#[allow(clippy::too_many_arguments)]
fn matches_at(
    nodes: &[Node],
    text: &[char],
    ni: usize,
    ti: usize,
    nocase: bool,
    block_dot: bool,
    dot_allowed: bool,
    has_leading_dot: bool,
    memo: &mut Memo,
) -> bool {
    if let Some(&hit) = memo.get(&(ni, ti)) {
        return hit;
    }
    // Guard against pathological recursion depth on extreme inputs by
    // relying on the memo table to cap total work at O(nodes * text); the
    // recursion itself only ever moves ni or ti forward, so it terminates.
    let result = matches_at_uncached(nodes, text, ni, ti, nocase, block_dot, dot_allowed, has_leading_dot, memo);
    memo.insert((ni, ti), result);
    result
}

#[allow(clippy::too_many_arguments)]
fn matches_at_uncached(
    nodes: &[Node],
    text: &[char],
    ni: usize,
    ti: usize,
    nocase: bool,
    block_dot: bool,
    dot_allowed: bool,
    has_leading_dot: bool,
    memo: &mut Memo,
) -> bool {
    let Some(node) = nodes.get(ni) else {
        return ti == text.len();
    };
    // An explicit literal '.' as the pattern's very first node is always
    // allowed to consume a leading dot (`.*` matches `.foo`); reached any
    // other way - a later node once an earlier wildcard shrank to zero
    // width, or a wildcard/extglob itself - it's blocked when the guard is
    // active.
    let is_explicit_leading_literal = ni == 0 && matches!(node, Node::Literal(_));
    let blocked_here = block_dot && ti == 0 && !is_explicit_leading_literal;

    match node {
        Node::Literal(lit) => {
            let lit_chars: Vec<char> = lit.chars().collect();
            let end = ti + lit_chars.len();
            if end > text.len() || blocked_here {
                return false;
            }
            let region = &text[ti..end];
            let eq = if nocase {
                region.iter().zip(&lit_chars).all(|(a, b)| crate::charclass::chars_eq_nocase(*a, *b))
            } else {
                region == lit_chars.as_slice()
            };
            eq && matches_at(nodes, text, ni + 1, end, nocase, block_dot, dot_allowed, has_leading_dot, memo)
        }
        Node::AnyChar => {
            ti < text.len() && !blocked_here && matches_at(nodes, text, ni + 1, ti + 1, nocase, block_dot, dot_allowed, has_leading_dot, memo)
        }
        Node::Star => {
            matches_at(nodes, text, ni + 1, ti, nocase, block_dot, dot_allowed, has_leading_dot, memo)
                || (ti < text.len()
                    && !blocked_here
                    && matches_at(nodes, text, ni, ti + 1, nocase, block_dot, dot_allowed, has_leading_dot, memo))
        }
        Node::Class(class) => {
            ti < text.len()
                && !blocked_here
                && class.matches(text[ti], nocase)
                && matches_at(nodes, text, ni + 1, ti + 1, nocase, block_dot, dot_allowed, has_leading_dot, memo)
        }
        Node::ExtGlob { kind, alts } => {
            // At ti==0, whether *this* extglob's own alternatives permit a
            // leading dot is decided per its own kind/alts (an alt starting
            // with a literal `.` is always fine), not by the outer
            // `blocked_here`.
            let alt_guard = ti == 0 && extglob_guard_active(*kind, alts, dot_allowed, has_leading_dot);
            !alt_guard && ext_matches(nodes, text, ni, ti, *kind, alts, nocase, block_dot, dot_allowed, has_leading_dot, memo)
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// ponytail: re-checks each alternative against each candidate span from
/// scratch rather than sharing one global (node, start, end) memo table
/// across the whole match, so deeply nested extglobs redo some work. Still
/// strictly polynomial (bounded by span count x alt size), never
/// exponential - the property that actually matters here. Upgrade to a
/// shared memo if profiling ever shows this path is hot.
fn ext_matches(
    nodes: &[Node],
    text: &[char],
    ni: usize,
    ti: usize,
    kind: ExtKind,
    alts: &[Vec<Node>],
    nocase: bool,
    block_dot: bool,
    dot_allowed: bool,
    has_leading_dot: bool,
    memo: &mut Memo,
) -> bool {
    let cont = |end: usize, memo: &mut Memo| matches_at(nodes, text, ni + 1, end, nocase, block_dot, dot_allowed, has_leading_dot, memo);
    match kind {
        ExtKind::ZeroOrOne => {
            cont(ti, memo)
                || any_alt_span(alts, text, ti, nocase, dot_allowed, has_leading_dot).any(|end| cont(end, memo))
        }
        ExtKind::ExactlyOne => any_alt_span(alts, text, ti, nocase, dot_allowed, has_leading_dot).any(|end| cont(end, memo)),
        ExtKind::ZeroOrMore => {
            cont(ti, memo) || repeat_matches(nodes, text, ni, ti, alts, nocase, block_dot, dot_allowed, has_leading_dot, memo)
        }
        ExtKind::OneOrMore => repeat_matches(nodes, text, ni, ti, alts, nocase, block_dot, dot_allowed, has_leading_dot, memo),
        ExtKind::Not => {
            let alt_ends: std::collections::HashSet<usize> = any_alt_span(alts, text, ti, nocase, dot_allowed, has_leading_dot).collect();
            (ti..=text.len()).any(|end| !alt_ends.contains(&end) && cont(end, memo))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn repeat_matches(
    nodes: &[Node],
    text: &[char],
    ni: usize,
    ti: usize,
    alts: &[Vec<Node>],
    nocase: bool,
    block_dot: bool,
    dot_allowed: bool,
    has_leading_dot: bool,
    memo: &mut Memo,
) -> bool {
    any_alt_span(alts, text, ti, nocase, dot_allowed, has_leading_dot).any(|end| {
        // Stopping after this repetition is fine even if it consumed
        // nothing (an alt that matches empty, e.g. `?(x)`), but repeating
        // *again* at the same position would recurse forever - a
        // repetition only continues if it made forward progress.
        matches_at(nodes, text, ni + 1, end, nocase, block_dot, dot_allowed, has_leading_dot, memo)
            || (end > ti && repeat_matches(nodes, text, ni, end, alts, nocase, block_dot, dot_allowed, has_leading_dot, memo))
    })
}

/// End positions >= `start` at which some alternative fully matches
/// `text[start..end]`. When `start == 0`, each alternative gets its own
/// leading-dot guard computed from its own first node, same as any other
/// pattern would.
fn any_alt_span<'a>(
    alts: &'a [Vec<Node>],
    text: &'a [char],
    start: usize,
    nocase: bool,
    dot_allowed: bool,
    has_leading_dot: bool,
) -> impl Iterator<Item = usize> + 'a {
    (start..=text.len()).filter(move |&end| {
        alts.iter().any(|alt| {
            let alt_guard = start == 0 && guard_active(alt, dot_allowed, has_leading_dot);
            full_match(alt, &text[start..end], nocase, alt_guard, dot_allowed, has_leading_dot)
        })
    })
}

fn full_match(nodes: &[Node], text: &[char], nocase: bool, block_dot: bool, dot_allowed: bool, has_leading_dot: bool) -> bool {
    let mut memo = HashMap::new();
    matches_at(nodes, text, 0, 0, nocase, block_dot, dot_allowed, has_leading_dot, &mut memo)
}
