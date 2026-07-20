use rs_minimatch_core::{minimatch, Minimatch, Options};

fn opts() -> Options {
    Options::default()
}

#[test]
fn basic_wildcards() {
    assert!(minimatch("foo.js", "*.js", opts()));
    assert!(!minimatch("foo.ts", "*.js", opts()));
    assert!(minimatch("foo/bar.js", "foo/*.js", opts()));
    assert!(minimatch("a", "?", opts()));
    assert!(!minimatch("ab", "?", opts()));
}

#[test]
fn globstar() {
    assert!(minimatch("a/b/c/d.js", "a/**/*.js", opts()));
    assert!(minimatch("a/d.js", "a/**/*.js", opts()));
    assert!(!minimatch("a/.d.js", "a/**/*.js", opts()));
    assert!(minimatch("a/.d.js", "a/**/*.js", Options { dot: true, ..opts() }));
}

#[test]
fn trailing_globstar_needs_something_to_sweep() {
    // "a/**" spells out a literal "/" - it must match "a/" and "a/b" (there's
    // something for the trailing ** to sweep, even zero-width for a bare
    // trailing slash) but not "a" alone, which never contains that "/" at
    // all. Verified against the real minimatch package directly.
    assert!(!minimatch("a", "a/**", opts()));
    assert!(minimatch("a/", "a/**", opts()));
    assert!(minimatch("a/b", "a/**", opts()));
    // A globstar with nothing before it has no such anchor requirement.
    assert!(minimatch("foo.js", "**", opts()));
    // Same anchor rule still has to hold with a literal *after* the
    // trailing run too, decided via a leading globstar with some slack.
    assert!(!minimatch("a", "**/a/**", opts()));
    assert!(minimatch("a/a", "**/a/**", opts()));
    assert!(!minimatch("x/a", "**/a/**", opts()));
}

#[test]
fn char_classes() {
    assert!(minimatch("a.js", "[a-c].js", opts()));
    assert!(!minimatch("d.js", "[a-c].js", opts()));
    assert!(minimatch("d.js", "[!a-c].js", opts()));
}

#[test]
fn extglobs() {
    assert!(minimatch("foo.js", "+(foo|bar).js", opts()));
    assert!(minimatch("foobar.js", "+(foo|bar).js", opts()));
    assert!(!minimatch("baz.js", "+(foo|bar).js", opts()));
    // A *leading* `!(...)` is always whole-pattern negation first in
    // minimatch (verified against the real package) - not the `!` extglob.
    // `x!(test).js` puts it in non-leading position to actually test the
    // extglob form.
    assert!(minimatch("test.js", "!(test).js", opts()));
    assert!(minimatch("xmain.js", "x!(test).js", opts()));
    assert!(!minimatch("xtest.js", "x!(test).js", opts()));
    assert!(minimatch(".js", "*(test).js", opts()));
    assert!(minimatch("test.js", "*(test).js", opts()));
}

#[test]
fn brace_in_pattern() {
    assert!(minimatch("a.js", "{a,b}.js", opts()));
    assert!(minimatch("b.js", "{a,b}.js", opts()));
    assert!(!minimatch("c.js", "{a,b}.js", opts()));
}

#[test]
fn negation() {
    assert!(!minimatch("foo.js", "!*.js", opts()));
    assert!(minimatch("foo.ts", "!*.js", opts()));
}

#[test]
fn minimatch_class_reuse() {
    let mm = Minimatch::new("src/**/*.rs", opts());
    assert!(mm.is_match("src/lib.rs"));
    assert!(mm.is_match("src/a/b/c.rs"));
    assert!(!mm.is_match("src/a/b/c.js"));
}

#[test]
fn no_redos_on_many_globstars() {
    let pattern = "**/".repeat(11) + "foo";
    let path = "a/".repeat(30) + "foo";
    let start = std::time::Instant::now();
    let _ = minimatch(&path, &pattern, opts());
    assert!(start.elapsed().as_millis() < 200, "took {:?}", start.elapsed());
}
