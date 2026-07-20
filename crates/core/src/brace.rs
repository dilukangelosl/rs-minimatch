//! Bash-style brace expansion (`{a,b,c}`, `{1..5}`, `{a..z}`, nesting),
//! ported from the `brace-expansion` npm package's algorithm (the one
//! `minimatch` itself depends on).
//!
//! Bounded on two axes, matching that package's fix for CVE-2026-14257 (a
//! real, published DoS: `'{a,b}'.repeat(1500)` stays under any *count* limit
//! while every result grows one character per repeat, so an unbounded
//! *count* cap alone still lets total output size explode and exhaust
//! memory). `MAX_EXPANSIONS` caps the number of results; `MAX_LENGTH` caps
//! total accumulated characters across all of them.

pub const MAX_EXPANSIONS: usize = 100_000;
pub const MAX_LENGTH: usize = 4_000_000;

const ESC_SLASH: char = '\u{E000}';
const ESC_OPEN: char = '\u{E001}';
const ESC_CLOSE: char = '\u{E002}';
const ESC_COMMA: char = '\u{E003}';
const ESC_PERIOD: char = '\u{E004}';

pub fn expand(pattern: &str) -> Vec<String> {
    expand_bounded(pattern, MAX_EXPANSIONS, MAX_LENGTH)
}

pub fn expand_bounded(pattern: &str, max: usize, max_length: usize) -> Vec<String> {
    if pattern.is_empty() {
        return Vec::new();
    }
    // Bash quirk (preserved for compatibility): a leading `{}` is treated
    // literally rather than as an (empty, non-expanding) brace group.
    let pattern = if let Some(rest) = pattern.strip_prefix("{}") {
        format!("\\{{\\}}{rest}")
    } else {
        pattern.to_string()
    };
    let escaped = escape_braces(&pattern);
    expand_inner(&escaped, max, max_length, true)
        .into_iter()
        .map(|s| unescape_braces(&s))
        .collect()
}

fn escape_braces(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            match chars[i + 1] {
                '\\' => {
                    out.push(ESC_SLASH);
                    i += 2;
                    continue;
                }
                '{' => {
                    out.push(ESC_OPEN);
                    i += 2;
                    continue;
                }
                '}' => {
                    out.push(ESC_CLOSE);
                    i += 2;
                    continue;
                }
                ',' => {
                    out.push(ESC_COMMA);
                    i += 2;
                    continue;
                }
                '.' => {
                    out.push(ESC_PERIOD);
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn unescape_braces(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ESC_SLASH => '\\',
            ESC_OPEN => '{',
            ESC_CLOSE => '}',
            ESC_COMMA => ',',
            ESC_PERIOD => '.',
            other => other,
        })
        .collect()
}

struct Balanced {
    pre: String,
    body: String,
    post: String,
}

/// Finds a top-level `{...}` group. A direct port of the `balanced-match`
/// npm package (which `brace-expansion` itself depends on) rather than a
/// naive leftmost-open/depth-tracking scan: with more `{` than can pair up,
/// bash (and this algorithm) matches the innermost viable pair and leaves
/// extra leading `{` characters as literal text, which a simple depth
/// counter gets wrong (verified against real bash's output on cases like
/// `"{{a,b}"` -> `["{a", "{b"]`, not a parse failure).
fn balanced(s: &str) -> Option<Balanced> {
    let chars: Vec<char> = s.chars().collect();
    let (start, end) = balanced_range(&chars)?;
    Some(Balanced {
        pre: chars[..start].iter().collect(),
        body: chars[start + 1..end].iter().collect(),
        post: chars[end + 1..].iter().collect(),
    })
}

fn index_of(chars: &[char], c: char, from: usize) -> Option<usize> {
    chars[from.min(chars.len())..].iter().position(|&x| x == c).map(|p| p + from.min(chars.len()))
}

fn balanced_range(chars: &[char]) -> Option<(usize, usize)> {
    let mut ai = index_of(chars, '{', 0);
    ai?;
    let mut bi = index_of(chars, '}', ai.unwrap() + 1);
    bi?;
    let mut i = ai;

    let mut begs: Vec<usize> = Vec::new();
    let mut left = chars.len();
    let mut right = 0usize;
    let mut result: Option<(usize, usize)> = None;

    while let Some(cur) = i {
        if result.is_some() {
            break;
        }
        if Some(cur) == ai {
            begs.push(cur);
            ai = index_of(chars, '{', cur + 1);
        } else if begs.len() == 1 {
            result = Some((begs.pop().unwrap(), bi.unwrap()));
        } else if let Some(beg) = begs.pop() {
            if beg < left {
                left = beg;
                right = bi.unwrap();
            }
            bi = index_of(chars, '}', cur + 1);
        }
        i = match (ai, bi) {
            (Some(a), Some(b)) if a < b => Some(a),
            (_, Some(b)) => Some(b),
            _ => None,
        };
    }

    if result.is_none() && !begs.is_empty() {
        result = Some((left, right));
    }
    result
}

/// `str.split(',')`, except a nested `{...}` group's commas don't count.
fn parse_comma_parts(s: &str) -> Vec<String> {
    if s.is_empty() {
        return vec![String::new()];
    }
    match balanced(s) {
        None => s.split(',').map(str::to_string).collect(),
        Some(b) => {
            let mut parts: Vec<String> = b.pre.split(',').map(str::to_string).collect();
            let last = parts.len() - 1;
            parts[last] = format!("{}{{{}}}", parts[last], b.body);
            if !b.post.is_empty() {
                let mut post_parts = parse_comma_parts(&b.post);
                let first = post_parts.remove(0);
                parts[last] = format!("{}{}", parts[last], first);
                parts.extend(post_parts);
            }
            parts
        }
    }
}

fn is_numeric_sequence(body: &str) -> bool {
    let parts: Vec<&str> = body.split("..").collect();
    if parts.len() != 2 && parts.len() != 3 {
        return false;
    }
    parts.iter().all(|p| is_signed_int(p))
}

fn is_signed_int(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn is_alpha_sequence(body: &str) -> bool {
    let parts: Vec<&str> = body.split("..").collect();
    if parts.len() != 2 && parts.len() != 3 {
        return false;
    }
    let is_single_alpha = |s: &str| s.chars().count() == 1 && s.chars().next().unwrap().is_ascii_alphabetic();
    if !is_single_alpha(parts[0]) || !is_single_alpha(parts[1]) {
        return false;
    }
    parts.len() == 2 || is_signed_int(parts[2])
}

fn is_padded(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    let bytes = s.as_bytes();
    bytes.len() >= 2 && bytes[0] == b'0' && bytes[1].is_ascii_digit()
}

fn numeric(s: &str) -> i64 {
    s.parse::<i64>().unwrap_or_else(|_| s.chars().next().map(|c| c as i64).unwrap_or(0))
}

fn expand_sequence(body: &str, is_alpha: bool, max: usize) -> Vec<String> {
    let parts: Vec<&str> = body.split("..").collect();
    if parts.len() < 2 {
        return Vec::new();
    }
    let x = numeric(parts[0]);
    let y = numeric(parts[1]);
    let width = parts[0].chars().count().max(parts[1].chars().count());
    let mut incr = if parts.len() == 3 {
        numeric(parts[2]).unsigned_abs().max(1) as i64
    } else {
        1
    };
    let reverse = y < x;
    if reverse {
        incr = -incr;
    }
    let pad = parts.iter().any(|p| is_padded(p));

    let mut out = Vec::new();
    let mut i = x;
    loop {
        if out.len() >= max {
            break;
        }
        let done = if reverse { i < y } else { i > y };
        if done {
            break;
        }
        let s = if is_alpha {
            let c = char::from_u32(i as u32).unwrap_or('\u{FFFD}');
            if c == '\\' {
                String::new()
            } else {
                c.to_string()
            }
        } else {
            let mut c = i.to_string();
            if pad {
                // `width` is the character length of the original bound
                // strings (e.g. "-01" -> 3), which already accounts for a
                // leading sign. So `need` is computed against the full
                // signed string's length, not just the digit portion.
                let need = width.saturating_sub(c.len());
                if need > 0 {
                    let zeros = "0".repeat(need);
                    c = if let Some(digits) = c.strip_prefix('-') {
                        format!("-{zeros}{digits}")
                    } else {
                        format!("{zeros}{c}")
                    };
                }
            }
            c
        };
        out.push(s);
        i += incr;
        if incr == 0 {
            break;
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn combine(acc: &[String], pre: &str, values: &[String], max: usize, max_length: usize, drop_empties: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut length = 0usize;
    for a in acc {
        for v in values {
            if out.len() >= max {
                return out;
            }
            let mut expansion = String::with_capacity(a.len() + pre.len() + v.len());
            expansion.push_str(a);
            expansion.push_str(pre);
            expansion.push_str(v);
            if drop_empties && expansion.is_empty() {
                continue;
            }
            if length + expansion.len() > max_length {
                return out;
            }
            length += expansion.len();
            out.push(expansion);
        }
    }
    out
}

fn expand_inner(input: &str, max: usize, max_length: usize, is_top: bool) -> Vec<String> {
    let mut acc = vec![String::new()];
    let mut drop_empties = false;
    let mut first_group = true;
    let mut str = input.to_string();
    let mut is_top = is_top;

    loop {
        let m = match balanced(&str) {
            None => return combine(&acc, &str, &[String::new()], max, max_length, drop_empties),
            Some(m) => m,
        };

        if m.pre.ends_with('$') {
            acc = combine(
                &acc,
                &format!("{}{{{}}}", m.pre, m.body),
                &[String::new()],
                max,
                max_length,
                drop_empties && m.post.is_empty(),
            );
            first_group = false;
            if m.post.is_empty() {
                break;
            }
            str = m.post;
            continue;
        }

        let is_numeric = is_numeric_sequence(&m.body);
        let is_alpha = is_alpha_sequence(&m.body);
        let is_sequence = is_numeric || is_alpha;
        let is_options = m.body.contains(',');

        if !is_sequence && !is_options {
            if has_unescaped_comma_before_close(&m.post) {
                str = format!("{}{{{}{}{}", m.pre, m.body, ESC_CLOSE, m.post);
                is_top = true;
                continue;
            }
            return combine(
                &acc,
                &format!("{}{{{}}}{}", m.pre, m.body, m.post),
                &[String::new()],
                max,
                max_length,
                drop_empties,
            );
        }

        if first_group {
            drop_empties = is_top && !is_sequence;
            first_group = false;
        }

        let values = if is_sequence {
            expand_sequence(&m.body, is_alpha, max)
        } else {
            let mut n = parse_comma_parts(&m.body);
            // A body with no top-level comma of its own (parseCommaParts
            // returns it whole, braces reattached, e.g. body "{a,b}" inside
            // "{{a,b}}") isn't itself an options group - it's an inner
            // group whose *result* should stay wrapped in literal braces,
            // hence the outer pair around it. Expand it, then re-wrap.
            if n.len() == 1 {
                n = expand_inner(&n[0], max, max_length, false)
                    .into_iter()
                    .map(|s| format!("{{{s}}}"))
                    .collect();
            }
            let mut values = Vec::new();
            for part in &n {
                values.extend(expand_inner(part, max, max_length, false));
            }
            values
        };

        acc = combine(&acc, &m.pre, &values, max, max_length, drop_empties && m.post.is_empty());
        if m.post.is_empty() {
            break;
        }
        str = m.post;
    }
    acc
}

fn has_unescaped_comma_before_close(post: &str) -> bool {
    let chars: Vec<char> = post.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ',' && chars.get(i + 1) != Some(&',') {
            return chars[i + 1..].contains(&'}');
        }
        i += 1;
    }
    false
}
