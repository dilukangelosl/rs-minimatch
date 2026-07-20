//! Zero-dependency Rust implementation of `minimatch`'s glob matching,
//! immune to catastrophic backtracking by construction: every matching
//! function here is memoized dynamic programming over bounded index pairs,
//! never a backtracking search over unbounded possibilities. See
//! `matcher.rs` and `path.rs` for where that guarantee actually lives.

mod api;
mod brace;
mod charclass;
mod escape;
mod matcher;
mod options;
mod pattern;
mod path;

pub use api::{brace_expand, filter, match_list, minimatch, Minimatch};
pub use brace::{expand as brace_expand_raw, expand_bounded as brace_expand_bounded, MAX_EXPANSIONS, MAX_LENGTH};
pub use charclass::{CharClass, ClassItem, PosixClass};
pub use escape::{escape, unescape};
pub use options::{Options, Platform};
pub use path::Segment;
pub use pattern::{ExtKind, Node};
