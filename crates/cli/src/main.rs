use std::process::ExitCode;

use rs_minimatch_core::{match_list, Options};

const HELP: &str = "\
rs-minimatch

Filter paths by a glob pattern, backed by rs-minimatch-core.

Usage: rs-minimatch [options] <pattern> <path> [<path> ...]
       <something-that-lists-paths> | rs-minimatch [options] <pattern>

Prints every given path that matches the pattern, one per line.
Exits 0 if at least one path matched, 1 otherwise.

Options:
  -d, --dot                Let patterns match dotfiles
  -b, --match-base          A pattern with no slash matches the basename only
  -i, --nocase              Case-insensitive matching
      --nobrace             Don't expand {a,b,c} braces
      --noext               Don't support extglob patterns (!(x), +(x), ...)
      --noglobstar          Treat ** like a plain *
      --nonegate             Don't support a leading ! for whole-pattern negation
  -v, --invert-match         Print paths that do NOT match instead
  -h, --help                 Show this help
      --version               Show the version

Reads paths from stdin (one per line) if none are given as arguments.";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut opts = Options::default();
    let mut invert = false;
    let mut positional = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                println!("{HELP}");
                return ExitCode::SUCCESS;
            }
            "--version" => {
                println!("rs-minimatch {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "-d" | "--dot" => opts.dot = true,
            "-b" | "--match-base" => opts.match_base = true,
            "-i" | "--nocase" => opts.nocase = true,
            "--nobrace" => opts.nobrace = true,
            "--noext" => opts.noext = true,
            "--noglobstar" => opts.noglobstar = true,
            "--nonegate" => opts.nonegate = true,
            "-v" | "--invert-match" => invert = true,
            other => positional.push(other.to_string()),
        }
        i += 1;
    }
    args.clear();

    let Some(pattern) = positional.first().cloned() else {
        eprintln!("rs-minimatch: missing <pattern>\n");
        println!("{HELP}");
        return ExitCode::FAILURE;
    };

    let paths: Vec<String> = if positional.len() > 1 {
        positional[1..].to_vec()
    } else {
        use std::io::BufRead;
        std::io::stdin().lock().lines().map_while(Result::ok).collect()
    };
    let path_refs: Vec<&str> = paths.iter().map(String::as_str).collect();

    let matched = match_list(&path_refs, &pattern, opts);
    let matched: std::collections::HashSet<&str> = matched.iter().map(String::as_str).collect();

    let mut any = false;
    for p in &paths {
        let is_match = matched.contains(p.as_str());
        if is_match != invert {
            println!("{p}");
            any = true;
        }
    }

    if any {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
