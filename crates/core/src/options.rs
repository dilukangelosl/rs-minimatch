/// Matching options, mirroring minimatch's `Options` interface.
#[derive(Debug, Clone)]
pub struct Options {
    pub dot: bool,
    pub match_base: bool,
    pub nobrace: bool,
    pub nocase: bool,
    pub noext: bool,
    pub nonegate: bool,
    pub nocomment: bool,
    pub noglobstar: bool,
    pub allow_windows_escape: bool,
    pub windows_paths_no_escape: bool,
    pub platform: Platform,
    pub partial: bool,
    pub flip_negate: bool,
    pub preserve_multiple_slashes: bool,
    /// `match_list`/`match()`: if nothing in the list matches, return the
    /// pattern itself instead of an empty result.
    pub nonull: bool,
    /// Caps recursion when matching `**` against many path segments, same
    /// safety valve minimatch itself uses (default matches theirs: 200).
    pub max_globstar_recursion: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Platform {
    #[default]
    Posix,
    Win32,
}

impl Options {
    pub fn is_windows(&self) -> bool {
        self.platform == Platform::Win32
    }
}

impl Default for Options {
    fn default() -> Self {
        Options {
            dot: false,
            match_base: false,
            nobrace: false,
            nocase: false,
            noext: false,
            nonegate: false,
            nocomment: false,
            noglobstar: false,
            allow_windows_escape: true,
            windows_paths_no_escape: false,
            platform: Platform::Posix,
            partial: false,
            flip_negate: false,
            preserve_multiple_slashes: false,
            nonull: false,
            max_globstar_recursion: 200,
        }
    }
}
