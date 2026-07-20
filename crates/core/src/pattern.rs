//! Parses one path segment (no `/` inside — the caller already split on
//! that) into an AST: literals, `?`, `*`, `[...]` classes, and the five
//! extglob forms. No regex is generated; `matcher.rs` interprets this
//! directly.

use crate::charclass::{self, CharClass};

#[derive(Debug, Clone)]
pub enum Node {
    Literal(String),
    AnyChar,
    Star,
    Class(CharClass),
    ExtGlob { kind: ExtKind, alts: Vec<Vec<Node>> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtKind {
    /// `!(...)` — matches anything except the alternatives.
    Not,
    /// `?(...)` — zero or one.
    ZeroOrOne,
    /// `*(...)` — zero or more.
    ZeroOrMore,
    /// `+(...)` — one or more.
    OneOrMore,
    /// `@(...)` — exactly one of the alternatives.
    ExactlyOne,
}

fn ext_kind(c: char) -> Option<ExtKind> {
    Some(match c {
        '!' => ExtKind::Not,
        '?' => ExtKind::ZeroOrOne,
        '*' => ExtKind::ZeroOrMore,
        '+' => ExtKind::OneOrMore,
        '@' => ExtKind::ExactlyOne,
        _ => return None,
    })
}

pub fn parse_segment(s: &str, noext: bool) -> Vec<Node> {
    let chars: Vec<char> = s.chars().collect();
    let mut p = Parser { chars: &chars, pos: 0, noext };
    p.parse_until(&[])
}

struct Parser<'a> {
    chars: &'a [char],
    pos: usize,
    noext: bool,
}

impl<'a> Parser<'a> {
    fn parse_until(&mut self, terminators: &[char]) -> Vec<Node> {
        let mut nodes = Vec::new();
        let mut literal = String::new();

        while self.pos < self.chars.len() {
            let c = self.chars[self.pos];
            if terminators.contains(&c) {
                break;
            }

            if c == '\\' {
                if self.pos == self.chars.len() - 1 {
                    // Trailing lone backslash: nothing to escape, literal.
                    literal.push('\\');
                    self.pos += 1;
                } else {
                    literal.push(self.chars[self.pos + 1]);
                    self.pos += 2;
                }
                continue;
            }

            if c == '[' {
                if let Some((class, end)) = charclass::parse(self.chars, self.pos) {
                    flush(&mut nodes, &mut literal);
                    nodes.push(Node::Class(class));
                    self.pos = end;
                    continue;
                }
                literal.push('[');
                self.pos += 1;
                continue;
            }

            if !self.noext && ext_kind(c).is_some() && self.chars.get(self.pos + 1) == Some(&'(') {
                let start = self.pos;
                let kind = ext_kind(c).unwrap();
                self.pos += 2;
                let mut alts = Vec::new();
                let mut terminated = false;
                loop {
                    alts.push(self.parse_until(&['|', ')']));
                    match self.chars.get(self.pos) {
                        Some('|') => {
                            self.pos += 1;
                        }
                        Some(')') => {
                            self.pos += 1;
                            terminated = true;
                            break;
                        }
                        _ => break,
                    }
                }
                if terminated {
                    flush(&mut nodes, &mut literal);
                    nodes.push(Node::ExtGlob { kind, alts });
                } else {
                    // Unterminated extglob: not magic after all, treat the
                    // whole thing (from the type char on) as literal text,
                    // same as minimatch's malformed-extglob fallback.
                    literal.extend(&self.chars[start..self.pos]);
                }
                continue;
            }

            if c == '*' {
                flush(&mut nodes, &mut literal);
                while self.chars.get(self.pos) == Some(&'*') {
                    self.pos += 1;
                }
                nodes.push(Node::Star);
                continue;
            }

            if c == '?' {
                flush(&mut nodes, &mut literal);
                nodes.push(Node::AnyChar);
                self.pos += 1;
                continue;
            }

            literal.push(c);
            self.pos += 1;
        }

        flush(&mut nodes, &mut literal);
        nodes
    }
}

fn flush(nodes: &mut Vec<Node>, literal: &mut String) {
    if !literal.is_empty() {
        nodes.push(Node::Literal(std::mem::take(literal)));
    }
}

/// Whether this segment pattern is exactly the literal string `.` or `..`
/// (glob traversal segments are never matched by magic, even under `dot`).
pub fn is_only_dots(nodes: &[Node]) -> bool {
    match nodes {
        [Node::Literal(s)] => s == "." || s == "..",
        _ => false,
    }
}

/// Whether this segment pattern's first character is a literal `.` (an
/// explicit request to match dotfiles at this position, regardless of the
/// `dot` option).
pub fn starts_with_literal_dot(nodes: &[Node]) -> bool {
    matches!(nodes.first(), Some(Node::Literal(s)) if s.starts_with('.'))
}
