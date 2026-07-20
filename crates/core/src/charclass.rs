//! `[...]` character class parsing, following minimatch's own
//! `brace-expressions.js` scan (bracket negation via `!`/`^`, ranges,
//! POSIX classes, the "leading `]` is literal" bash convention) but
//! building a directly-evaluable `CharClass` instead of a regex fragment,
//! since this crate's matcher never compiles to regex.

#[derive(Debug, Clone)]
pub struct CharClass {
    pub negate: bool,
    pub items: Vec<ClassItem>,
}

#[derive(Debug, Clone)]
pub enum ClassItem {
    Char(char),
    Range(char, char),
    Posix(PosixClass),
}

#[derive(Debug, Clone, Copy)]
pub enum PosixClass {
    Alnum,
    Alpha,
    Ascii,
    Blank,
    Cntrl,
    Digit,
    Graph,
    Lower,
    Print,
    Punct,
    Space,
    Upper,
    Word,
    Xdigit,
}

impl PosixClass {
    /// ponytail: uses Rust's `char` classification (ASCII/Unicode general
    /// categories) rather than porting minimatch's exact `\p{...}` regex
    /// fragments byte-for-byte. Matches on the common cases; edge-of-Unicode
    /// classification could differ from JS's regex engine in rare cases.
    fn matches(self, c: char) -> bool {
        match self {
            PosixClass::Alnum => c.is_alphanumeric(),
            PosixClass::Alpha => c.is_alphabetic(),
            PosixClass::Ascii => c.is_ascii(),
            PosixClass::Blank => c == ' ' || c == '\t' || c.is_whitespace() && !"\n\r\u{b}\u{c}".contains(c),
            PosixClass::Cntrl => c.is_control(),
            PosixClass::Digit => c.is_ascii_digit() || c.is_numeric(),
            PosixClass::Graph => !c.is_whitespace() && !c.is_control() && c != '\u{0}',
            PosixClass::Lower => c.is_lowercase(),
            PosixClass::Print => !c.is_control(),
            PosixClass::Punct => c.is_ascii_punctuation(),
            PosixClass::Space => c.is_whitespace(),
            PosixClass::Upper => c.is_uppercase(),
            PosixClass::Word => c.is_alphanumeric() || c == '_',
            PosixClass::Xdigit => c.is_ascii_hexdigit(),
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "alnum" => PosixClass::Alnum,
            "alpha" => PosixClass::Alpha,
            "ascii" => PosixClass::Ascii,
            "blank" => PosixClass::Blank,
            "cntrl" => PosixClass::Cntrl,
            "digit" => PosixClass::Digit,
            "graph" => PosixClass::Graph,
            "lower" => PosixClass::Lower,
            "print" => PosixClass::Print,
            "punct" => PosixClass::Punct,
            "space" => PosixClass::Space,
            "upper" => PosixClass::Upper,
            "word" => PosixClass::Word,
            "xdigit" => PosixClass::Xdigit,
            _ => return None,
        })
    }
}

/// Full Unicode case folding (not ASCII-only), so e.g. nocase matching
/// treats 'å' and 'Å' as equal, not just 'a'/'A'. `to_lowercase()` can
/// expand to more than one char for a handful of codepoints; comparing the
/// first is a reasonable approximation for glob matching purposes.
pub(crate) fn chars_eq_nocase(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

impl ClassItem {
    fn matches(&self, c: char, nocase: bool) -> bool {
        match self {
            ClassItem::Char(x) => *x == c || (nocase && chars_eq_nocase(*x, c)),
            ClassItem::Range(lo, hi) => (*lo..=*hi).contains(&c) || (nocase && (lower(*lo)..=lower(*hi)).contains(&lower(c))),
            ClassItem::Posix(p) => p.matches(c),
        }
    }
}

impl CharClass {
    pub fn matches(&self, c: char, nocase: bool) -> bool {
        let hit = self.items.iter().any(|i| i.matches(c, nocase));
        hit != self.negate
    }
}

/// Parses a `[...]` class starting at `chars[start] == '['`. Returns the
/// class and the index just past the closing `]`, or `None` if there's no
/// valid close (caller should then treat `[` as a literal character).
pub fn parse(chars: &[char], start: usize) -> Option<(CharClass, usize)> {
    debug_assert_eq!(chars.get(start), Some(&'['));
    let mut items = Vec::new();
    let mut i = start + 1;
    let mut saw_start = false;
    let mut escaping = false;
    let mut negate = false;
    let mut range_start: Option<char> = None;
    let mut end = None;

    if matches!(chars.get(i), Some('!') | Some('^')) {
        negate = true;
        i += 1;
    }

    while i < chars.len() {
        let c = chars[i];
        if c == ']' && saw_start && !escaping {
            end = Some(i);
            break;
        }
        saw_start = true;

        if c == '\\' && !escaping {
            escaping = true;
            i += 1;
            continue;
        }

        if c == '[' && !escaping {
            if let Some((posix, consumed)) = try_posix_class(chars, i) {
                if range_start.is_some() {
                    // `[a-[:alpha:]]` is invalid; the whole class fails.
                    return None;
                }
                items.push(ClassItem::Posix(posix));
                i += consumed;
                continue;
            }
        }

        escaping = false;

        if let Some(lo) = range_start.take() {
            if c > lo {
                items.push(ClassItem::Range(lo, c));
            } else if c == lo {
                items.push(ClassItem::Char(c));
            }
            // else: invalid (reversed) range, just drop it, like minimatch does.
            i += 1;
            continue;
        }

        // c-] means literal "c-"; c-<other> starts a range. Only consume
        // through the '-', leaving the ']' itself for the terminator check
        // on the next loop iteration - consuming it here too would let the
        // class run off the end looking for a close that already passed.
        if chars.get(i + 1) == Some(&'-') && chars.get(i + 2) == Some(&']') {
            items.push(ClassItem::Char(c));
            items.push(ClassItem::Char('-'));
            i += 2;
            continue;
        }
        if chars.get(i + 1) == Some(&'-') && chars.get(i + 2).is_some() {
            range_start = Some(c);
            i += 2;
            continue;
        }

        items.push(ClassItem::Char(c));
        i += 1;
    }

    let end = end?;
    if items.is_empty() {
        // No positive content at all (e.g. a stray `-` with nothing to
        // range against): matches nothing, same as minimatch's `$.`.
        return Some((CharClass { negate: false, items }, end + 1));
    }
    Some((CharClass { negate, items }, end + 1))
}

fn try_posix_class(chars: &[char], i: usize) -> Option<(PosixClass, usize)> {
    // Expects chars[i] == '[' and content shaped like "[:name:]".
    if chars.get(i + 1) != Some(&':') {
        return None;
    }
    let mut j = i + 2;
    let mut name = String::new();
    while j < chars.len() && chars[j] != ':' {
        name.push(chars[j]);
        j += 1;
    }
    if chars.get(j) == Some(&':') && chars.get(j + 1) == Some(&']') {
        PosixClass::from_name(&name).map(|p| (p, j + 2 - i))
    } else {
        None
    }
}
