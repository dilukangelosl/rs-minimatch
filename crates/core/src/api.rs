use std::collections::HashSet;

use crate::options::Options;
use crate::path::{self, Segment};

#[derive(Debug, Clone)]
pub struct Minimatch {
    pub pattern: String,
    pub options: Options,
    pub negate: bool,
    pub comment: bool,
    pub empty: bool,
    /// Brace-expanded, deduplicated pattern strings.
    pub glob_set: Vec<String>,
    /// Compiled per-alternative segment sets, one per `glob_set` entry.
    pub set: Vec<Vec<Segment>>,
}

impl Minimatch {
    pub fn new(pattern: &str, options: Options) -> Self {
        let mut pattern = pattern.to_string();
        if options.windows_paths_no_escape {
            pattern = pattern.replace('\\', "/");
        }

        if !options.nocomment && pattern.starts_with('#') {
            return Minimatch {
                pattern,
                options,
                negate: false,
                comment: true,
                empty: false,
                glob_set: vec![],
                set: vec![],
            };
        }
        if pattern.is_empty() {
            return Minimatch {
                pattern,
                options,
                negate: false,
                comment: false,
                empty: true,
                glob_set: vec![],
                set: vec![],
            };
        }

        let mut negate = false;
        let mut offset = 0;
        if !options.nonegate {
            for c in pattern.chars() {
                if c == '!' {
                    negate = !negate;
                    offset += 1;
                } else {
                    break;
                }
            }
        }
        let core_pattern: String = pattern.chars().skip(offset).collect();

        let expanded = brace_expand(&core_pattern, &options);
        let mut seen = HashSet::new();
        let glob_set: Vec<String> = expanded.into_iter().filter(|s| seen.insert(s.clone())).collect();
        let set: Vec<Vec<Segment>> = glob_set.iter().map(|p| path::compile_segments(p, &options)).collect();

        Minimatch {
            pattern,
            options,
            negate,
            comment: false,
            empty: false,
            glob_set,
            set,
        }
    }

    pub fn is_match(&self, file: &str) -> bool {
        self.is_match_partial(file, self.options.partial)
    }

    pub fn is_match_partial(&self, file: &str, partial: bool) -> bool {
        if self.comment {
            return false;
        }
        if self.empty {
            return file.is_empty();
        }
        if file == "/" && partial {
            return true;
        }

        let f = if self.options.is_windows() { file.replace('\\', "/") } else { file.to_string() };
        let file_segments = path::split_path(&f, self.options.preserve_multiple_slashes);
        let base = path::basename(&file_segments).to_string();

        for pat in &self.set {
            let use_file = if self.options.match_base && pat.len() == 1 { vec![base.clone()] } else { file_segments.clone() };
            if path::match_segments(pat, &use_file, &self.options, partial) {
                return if self.options.flip_negate { true } else { !self.negate };
            }
        }
        if self.options.flip_negate {
            false
        } else {
            self.negate
        }
    }

    /// Whether this pattern contains anything beyond literal text.
    pub fn has_magic(&self) -> bool {
        self.set.iter().any(|p| p.iter().any(|s| !matches!(s, Segment::Pattern(nodes) if is_plain_literal(nodes))))
    }
}

fn is_plain_literal(nodes: &[crate::pattern::Node]) -> bool {
    matches!(nodes, [crate::pattern::Node::Literal(_)]) || nodes.is_empty()
}

pub fn brace_expand(pattern: &str, options: &Options) -> Vec<String> {
    if options.nobrace {
        vec![pattern.to_string()]
    } else {
        crate::brace::expand(pattern)
    }
}

pub fn minimatch(path: &str, pattern: &str, options: Options) -> bool {
    if !options.nocomment && pattern.starts_with('#') {
        return false;
    }
    Minimatch::new(pattern, options).is_match(path)
}

pub fn filter(pattern: &str, options: Options) -> impl Fn(&str) -> bool {
    let mm = Minimatch::new(pattern, options);
    move |p: &str| mm.is_match(p)
}

pub fn match_list(list: &[&str], pattern: &str, options: Options) -> Vec<String> {
    let mm = Minimatch::new(pattern, options);
    list.iter().filter(|f| mm.is_match(f)).map(|s| s.to_string()).collect()
}
