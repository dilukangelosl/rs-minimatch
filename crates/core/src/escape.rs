//! Escaping glob-magic characters in a literal path, and reversing it.
//! Ported from minimatch's `escape.js`/`unescape.js`.

const MAGIC: &[char] = &['?', '*', '(', ')', '[', ']'];
const MAGIC_WITH_BRACES: &[char] = &['?', '*', '(', ')', '[', ']', '{', '}'];

pub fn escape(s: &str, windows_paths_no_escape: bool, magical_braces: bool) -> String {
    let magic: &[char] = if magical_braces { MAGIC_WITH_BRACES } else { MAGIC };
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if magic.contains(&c) {
            if windows_paths_no_escape {
                out.push('[');
                out.push(c);
                out.push(']');
            } else {
                out.push('\\');
                out.push(c);
            }
        } else if c == '\\' && !windows_paths_no_escape {
            out.push('\\');
            out.push('\\');
        } else {
            out.push(c);
        }
    }
    out
}

pub fn unescape(s: &str, windows_paths_no_escape: bool, magical_braces: bool) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        // `[x]` -> `x`, for a single non-slash, non-brace(-unless-magical) char.
        if chars[i] == '[' && i + 2 < chars.len() + 1 {
            if let Some(&close_candidate) = chars.get(i + 2) {
                if close_candidate == ']' {
                    let inner = chars[i + 1];
                    let ok = inner != '/' && inner != '\\' && (magical_braces || (inner != '{' && inner != '}'));
                    if ok {
                        out.push(inner);
                        i += 3;
                        continue;
                    }
                }
            }
        }
        if !windows_paths_no_escape && chars[i] == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if next != '/' && (magical_braces || (next != '{' && next != '}')) {
                out.push(next);
                i += 2;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}
