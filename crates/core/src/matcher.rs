//! Matching, guaranteed polynomial-time by construction: everything here is
//! memoized dynamic programming over (pattern position, text position)
//! pairs, never backtracking search. This is what makes the crate immune to
//! catastrophic blowup on adversarial input, the same property a regex
//! engine gets from *not* doing naive backtracking.

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
    let mut memo = Memos::for_shape(nodes, nodes.len(), chars.len());
    matches_at(nodes, &chars, 0, 0, nocase, block_dot, dot, has_leading_dot, &mut memo)
}

/// A single `*` on its own can't blow up without memoization (there's only
/// ever one place the "how many characters did it eat" choice branches, so
/// unmemoized work is still bounded by `O(nodes * text)`) - the catastrophic
/// case needs *multiple* stars re-trying the same span from different
/// entry points, or a quantified extglob revisiting the same split in more
/// than one way (`+(a|aa)` against a long run of `a`s is the textbook
/// example). So memoization is only ever skippable when there's no extglob
/// at all and at most one star.
fn needs_memo_table(nodes: &[Node]) -> bool {
    if nodes.iter().any(|n| matches!(n, Node::ExtGlob { .. })) {
        return true;
    }
    nodes.iter().filter(|n| matches!(n, Node::Star)).count() > 1
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

/// Memoization table keyed on (node index, text index). Two backends:
/// `Table` is a flat `Vec` sized once up front (no hashing, no
/// incremental-growth reallocation, unlike a `HashMap`); `Skip` does no
/// caching at all and allocates nothing, for the shapes `needs_memo_table`
/// has proven don't need it - a plain literal or single-`*` pattern like
/// `*.ts`, which is the overwhelming majority of real-world glob segments.
/// Same recursive algorithm, same result, either way - this only changes
/// how (or whether) a repeated `(ni, ti)` gets cached.
enum Memo {
    Skip,
    Table { cells: Vec<Option<bool>>, cols: usize },
}

impl Memo {
    fn for_shape(nodes: &[Node], rows: usize, cols: usize) -> Self {
        if needs_memo_table(nodes) {
            let cols = cols + 1;
            Memo::Table { cells: vec![None; (rows + 1) * cols], cols }
        } else {
            Memo::Skip
        }
    }

    fn get(&self, ni: usize, ti: usize) -> Option<bool> {
        match self {
            Memo::Skip => None,
            Memo::Table { cells, cols } => cells[ni * cols + ti],
        }
    }

    fn set(&mut self, ni: usize, ti: usize, value: bool) {
        if let Memo::Table { cells, cols } = self {
            cells[ni * *cols + ti] = Some(value);
        }
    }
}

/// Two independent tables sharing one (node index, text index) key shape
/// but answering different questions, so they can't share one cache:
/// `calls` is `matches_at`'s own "does the pattern from here match text
/// from here" memo. `repeats` is `repeat_matches`'s "can the */+ extglob
/// rooted at this node consume text from here onward, across any number of
/// repetitions" memo - see the comment on `repeat_matches` for why it needs
/// one at all.
struct Memos {
    calls: Memo,
    repeats: Memo,
}

impl Memos {
    fn for_shape(nodes: &[Node], rows: usize, cols: usize) -> Self {
        Memos { calls: Memo::for_shape(nodes, rows, cols), repeats: Memo::for_shape(nodes, rows, cols) }
    }
}

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
    memo: &mut Memos,
) -> bool {
    if let Some(hit) = memo.calls.get(ni, ti) {
        return hit;
    }
    // Guard against pathological recursion depth on extreme inputs by
    // relying on the memo table to cap total work at O(nodes * text); the
    // recursion itself only ever moves ni or ti forward, so it terminates.
    let result = matches_at_uncached(nodes, text, ni, ti, nocase, block_dot, dot_allowed, has_leading_dot, memo);
    memo.calls.set(ni, ti, result);
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
    memo: &mut Memos,
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
    memo: &mut Memos,
) -> bool {
    let cont = |end: usize, memo: &mut Memos| matches_at(nodes, text, ni + 1, end, nocase, block_dot, dot_allowed, has_leading_dot, memo);
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
/// "Can the `*(x)`/`+(x)` repetition rooted at `ni` consume `text[ti..]`
/// across any number of repetitions?" This recurses on *itself* (not
/// through `matches_at`), so without its own memo it's the textbook
/// unmemoized word-break blowup: `+(a|aa)` against a long run of `a`s can
/// reach the same (ni, ti) state through many different ways of splitting
/// the run into 1s and 2s, and every one of them re-explores the entire
/// rest of the string from scratch. Memoizing on (ni, ti) here - the same
/// key shape `matches_at` uses, but a different question, hence the
/// separate `repeats` table - caps it at the same O(nodes * text) bound as
/// everything else in this file.
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
    memo: &mut Memos,
) -> bool {
    if let Some(hit) = memo.repeats.get(ni, ti) {
        return hit;
    }
    let result = any_alt_span(alts, text, ti, nocase, dot_allowed, has_leading_dot).any(|end| {
        // Stopping after this repetition is fine even if it consumed
        // nothing (an alt that matches empty, e.g. `?(x)`), but repeating
        // *again* at the same position would recurse forever - a
        // repetition only continues if it made forward progress.
        matches_at(nodes, text, ni + 1, end, nocase, block_dot, dot_allowed, has_leading_dot, memo)
            || (end > ti && repeat_matches(nodes, text, ni, end, alts, nocase, block_dot, dot_allowed, has_leading_dot, memo))
    });
    memo.repeats.set(ni, ti, result);
    result
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
    let mut memo = Memos::for_shape(nodes, nodes.len(), text.len());
    matches_at(nodes, text, 0, 0, nocase, block_dot, dot_allowed, has_leading_dot, &mut memo)
}
