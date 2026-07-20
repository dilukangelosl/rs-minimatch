//! Thin NAPI-RS wrapper: every function here just marshals JS <-> Rust
//! types and delegates to `rs_minimatch_core`. No matching logic lives here.

use napi_derive::napi;
use rs_minimatch_core as core;

#[napi(object)]
#[derive(Default, Clone)]
pub struct MinimatchOptions {
    pub dot: Option<bool>,
    pub match_base: Option<bool>,
    pub nobrace: Option<bool>,
    pub nocase: Option<bool>,
    pub noext: Option<bool>,
    pub nonegate: Option<bool>,
    pub nocomment: Option<bool>,
    pub noglobstar: Option<bool>,
    pub nonull: Option<bool>,
    pub partial: Option<bool>,
    pub windows_paths_no_escape: Option<bool>,
    pub preserve_multiple_slashes: Option<bool>,
    pub flip_negate: Option<bool>,
    pub platform: Option<String>,
}

fn opts(o: Option<MinimatchOptions>) -> core::Options {
    let o = o.unwrap_or_default();
    core::Options {
        dot: o.dot.unwrap_or(false),
        match_base: o.match_base.unwrap_or(false),
        nobrace: o.nobrace.unwrap_or(false),
        nocase: o.nocase.unwrap_or(false),
        noext: o.noext.unwrap_or(false),
        nonegate: o.nonegate.unwrap_or(false),
        nocomment: o.nocomment.unwrap_or(false),
        noglobstar: o.noglobstar.unwrap_or(false),
        nonull: o.nonull.unwrap_or(false),
        partial: o.partial.unwrap_or(false),
        windows_paths_no_escape: o.windows_paths_no_escape.unwrap_or(false),
        preserve_multiple_slashes: o.preserve_multiple_slashes.unwrap_or(false),
        flip_negate: o.flip_negate.unwrap_or(false),
        platform: if o.platform.as_deref() == Some("win32") { core::Platform::Win32 } else { core::Platform::Posix },
        ..core::Options::default()
    }
}

fn refs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

#[napi]
pub struct Minimatch {
    inner: core::Minimatch,
}

#[napi]
impl Minimatch {
    #[napi(constructor)]
    pub fn new(pattern: String, options: Option<MinimatchOptions>) -> Self {
        Minimatch { inner: core::Minimatch::new(&pattern, opts(options)) }
    }

    #[napi(getter)]
    pub fn pattern(&self) -> String {
        self.inner.pattern.clone()
    }

    #[napi(getter)]
    pub fn negate(&self) -> bool {
        self.inner.negate
    }

    #[napi(getter)]
    pub fn comment(&self) -> bool {
        self.inner.comment
    }

    #[napi(getter)]
    pub fn empty(&self) -> bool {
        self.inner.empty
    }

    #[napi(getter, js_name = "globSet")]
    pub fn glob_set(&self) -> Vec<String> {
        self.inner.glob_set.clone()
    }

    #[napi]
    pub fn has_magic(&self) -> bool {
        self.inner.has_magic()
    }

    #[napi(js_name = "match")]
    pub fn matches(&self, path: String) -> bool {
        self.inner.is_match(&path)
    }
}

#[napi]
pub fn minimatch(path: String, pattern: String, options: Option<MinimatchOptions>) -> bool {
    core::minimatch(&path, &pattern, opts(options))
}

#[napi(js_name = "match")]
pub fn match_fn(list: Vec<String>, pattern: String, options: Option<MinimatchOptions>) -> Vec<String> {
    core::match_list(&refs(&list), &pattern, opts(options))
}

#[napi(js_name = "braceExpand")]
pub fn brace_expand(pattern: String, options: Option<MinimatchOptions>) -> Vec<String> {
    core::brace_expand(&pattern, &opts(options))
}

#[napi]
pub fn escape(s: String, windows_paths_no_escape: Option<bool>, magical_braces: Option<bool>) -> String {
    core::escape(&s, windows_paths_no_escape.unwrap_or(false), magical_braces.unwrap_or(false))
}

#[napi]
pub fn unescape(s: String, windows_paths_no_escape: Option<bool>, magical_braces: Option<bool>) -> String {
    core::unescape(&s, windows_paths_no_escape.unwrap_or(false), magical_braces.unwrap_or(true))
}
