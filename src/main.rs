// Placeholder main.rs for CLI interface
// Will be implemented in Phase 6

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::{Component, Path, PathBuf};
use tokio;

// Use the CLI wrapper for clean access to CLI functionality
use loregrep::cli_main::{AnalyzeArgs, CliApp, CliConfig, ExecToolArgs, ScanArgs, SearchArgs};

#[derive(Parser)]
#[command(name = "loregrep")]
#[command(about = "Lightweight Code Analysis Tool with AI-powered search")]
#[command(
    long_about = "Loregrep analyzes codebases using tree-sitter parsing and provides AI-powered natural language search capabilities"
)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Base directory for all path arguments. Relative paths given to a
    /// subcommand are resolved against it; absolute ones are used as-is. This
    /// does not change the process working directory.
    #[arg(short, long, global = true, default_value = ".")]
    pub directory: PathBuf,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Disable colored output
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Configuration file path
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Scan a repository for code analysis
    Scan(ScanArgs),
    /// Search for functions, structs, or other code elements
    Search(SearchArgs),
    /// Analyze a specific file
    Analyze(AnalyzeArgs),
    /// Execute a single analysis tool and print its JSON result (for agents)
    ExecTool(ExecToolArgs),
    /// Show current configuration
    Config,
}

/// Resolve a subcommand's path argument against the global `-d/--directory`.
///
/// One rule, applied identically by every subcommand:
///
/// * an **absolute** path argument is used verbatim — an explicit path always
///   wins over the global default;
/// * a **relative** path argument is joined onto `--directory`, then `.`
///   components are collapsed lexically.
///
/// The collapsing is what makes the default honest: `.`, `./` and `.//` are the
/// same directory, so `-d /repo` means `/repo` for all of them.
///
/// This replaces an equality test against the literal `PathBuf::from(".")`, under
/// which `-d` applied *only* when the argument was the current directory and was
/// ignored for every other relative path — so `-d /repo scan src` scanned `src`
/// beneath the caller's shell, not beneath `/repo`, with no diagnostic. (The old
/// test did handle `./` and `.//`, since `PathBuf`'s `PartialEq` compares
/// components; the hole was every *non*-default relative path.)
///
/// Joining (rather than "explicit relative path replaces the base") is chosen so
/// that the CLI agrees with the tool layer: `internal::paths::resolve_within_root`
/// already resolves every agent-supplied relative path against the analysis
/// root, never against the process working directory. With this rule
/// `loregrep -d /repo analyze src/lib.rs` and
/// `loregrep -d /repo exec-tool analyze_file --params '{"file_path":"src/lib.rs"}'`
/// name the same file, which is the property a tool-composing agent depends on.
///
/// Nothing here touches the filesystem: it is a pure lexical rule, so it behaves
/// the same for paths that do not exist yet and can be tested without fixtures.
fn effective_path(directory: &Path, arg: &Path) -> PathBuf {
    if arg.is_absolute() {
        return normalize_cur_dirs(arg);
    }
    let joined = directory.join(arg);
    let normalized = normalize_cur_dirs(&joined);
    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

/// Drop `.` components lexically. `..` is deliberately preserved: unlike `.`, it
/// is not a no-op in the presence of symlinks, and the layers below (and
/// `resolve_within_root`) are the right place to decide whether an escaping path
/// is acceptable.
fn normalize_cur_dirs(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // Load configuration
    let config = CliConfig::load(cli.config.as_deref())?;

    // Initialize CLI app
    let mut app = CliApp::new(config, cli.verbose, !cli.no_color).await?;

    // Execute command
    match cli.command {
        // Every subcommand resolves its path argument the same way; see
        // `effective_path`.
        Commands::Scan(mut args) => {
            args.path = effective_path(&cli.directory, &args.path);
            app.scan(args).await
        }
        Commands::Search(mut args) => {
            args.path = effective_path(&cli.directory, &args.path);
            app.search(args).await
        }
        Commands::Analyze(mut args) => {
            args.file = effective_path(&cli.directory, &args.file);
            app.analyze(args).await
        }
        Commands::ExecTool(mut args) => {
            // Only `--path` (the analysis root) is adjusted here. Path
            // parameters inside `--params` are left untouched on purpose: the
            // tool layer resolves them against this root via
            // `internal::paths::resolve_within_root`, and resolving them twice
            // would be wrong.
            args.path = effective_path(&cli.directory, &args.path);
            app.exec_tool(args).await
        }
        Commands::Config => app.show_config().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn eff(dir: &str, arg: &str) -> String {
        effective_path(Path::new(dir), Path::new(arg))
            .display()
            .to_string()
    }

    // --- K10: equivalent spellings of the default must behave identically ----

    #[test]
    fn every_spelling_of_the_current_directory_yields_the_base_directory() {
        // Every way of writing "the current directory" must land on the base
        // directory, including the ones a shell's tab-completion produces.
        for spelling in [".", "./", ".//", "./."] {
            assert_eq!(eff("/repo", spelling), "/repo", "spelling {spelling:?}");
        }
    }

    #[test]
    fn an_explicitly_written_dot_is_not_special_cased_away() {
        // `-d` defaults to "."; with no `-d` the result must still be usable.
        assert_eq!(eff(".", "."), ".");
        assert_eq!(eff(".", "./"), ".");
    }

    #[test]
    fn a_relative_argument_resolves_against_the_base_directory() {
        assert_eq!(eff("/repo", "src"), "/repo/src");
        assert_eq!(eff("/repo", "./src/lib.rs"), "/repo/src/lib.rs");
        assert_eq!(eff("/repo/", "src/"), "/repo/src");
    }

    #[test]
    fn an_absolute_argument_wins_over_the_base_directory() {
        assert_eq!(eff("/repo", "/elsewhere/src"), "/elsewhere/src");
        assert_eq!(eff("/repo", "/elsewhere/./src"), "/elsewhere/src");
    }

    #[test]
    fn a_relative_base_directory_is_kept_relative() {
        assert_eq!(eff("sub", "src"), "sub/src");
        assert_eq!(eff("./sub", "./src"), "sub/src");
    }

    #[test]
    fn parent_components_are_preserved_for_the_layers_below_to_judge() {
        // `..` is not a lexical no-op through symlinks, and containment is
        // enforced by internal::paths, not here.
        assert_eq!(eff("/repo", "../other"), "/repo/../other");
        assert_eq!(eff("/repo", "./../other"), "/repo/../other");
    }

    #[test]
    fn the_function_is_idempotent_on_its_own_output() {
        let once = effective_path(Path::new("/repo"), Path::new("./src/"));
        let twice = effective_path(Path::new("/repo"), &once);
        assert_eq!(once, twice);
    }

    // --- F10: analyze and exec-tool agree about relative paths --------------

    #[test]
    fn analyze_and_exec_tool_agree_on_the_same_relative_argument() {
        // The whole point of F10: the same relative text must name the same
        // place whichever entry point an agent reaches for.
        let cli =
            Cli::try_parse_from(["loregrep", "-d", "/repo", "analyze", "src/lib.rs"]).unwrap();
        let analyzed = match cli.command {
            Commands::Analyze(a) => effective_path(&cli.directory, &a.file),
            _ => unreachable!(),
        };

        let cli = Cli::try_parse_from([
            "loregrep",
            "-d",
            "/repo",
            "exec-tool",
            "analyze_file",
            "--path",
            "src",
        ])
        .unwrap();
        let root = match cli.command {
            Commands::ExecTool(a) => effective_path(&cli.directory, &a.path),
            _ => unreachable!(),
        };

        // exec-tool's root and analyze's file are rooted at the same base, so a
        // `file_path` param of "lib.rs" under that root names the analyzed file.
        assert_eq!(root, PathBuf::from("/repo/src"));
        assert_eq!(analyzed, root.join("lib.rs"));
    }

    #[test]
    fn scan_search_analyze_and_exec_tool_share_one_rule() {
        let base = "/repo";
        let cases: Vec<(&str, PathBuf)> = vec![
            (
                "scan",
                match Cli::try_parse_from(["loregrep", "-d", base, "scan", "./src"])
                    .unwrap()
                    .command
                {
                    Commands::Scan(a) => effective_path(Path::new(base), &a.path),
                    _ => unreachable!(),
                },
            ),
            (
                "search",
                match Cli::try_parse_from(["loregrep", "-d", base, "search", "q", "-p", "./src"])
                    .unwrap()
                    .command
                {
                    Commands::Search(a) => effective_path(Path::new(base), &a.path),
                    _ => unreachable!(),
                },
            ),
            (
                "analyze",
                match Cli::try_parse_from(["loregrep", "-d", base, "analyze", "./src"])
                    .unwrap()
                    .command
                {
                    Commands::Analyze(a) => effective_path(Path::new(base), &a.file),
                    _ => unreachable!(),
                },
            ),
            (
                "exec-tool",
                match Cli::try_parse_from([
                    "loregrep",
                    "-d",
                    base,
                    "exec-tool",
                    "t",
                    "--path",
                    "./src",
                ])
                .unwrap()
                .command
                {
                    Commands::ExecTool(a) => effective_path(Path::new(base), &a.path),
                    _ => unreachable!(),
                },
            ),
        ];
        for (name, resolved) in cases {
            assert_eq!(resolved, PathBuf::from("/repo/src"), "subcommand {name}");
        }
    }

    // --- K10: the dead scan flags are gone from the CLI surface -------------

    #[test]
    fn scan_no_longer_accepts_flags_it_would_discard() {
        // include/exclude/follow-symlinks were never read anywhere in src/;
        // patterns come from the config file. Rejecting them is honest.
        for flag in [
            vec!["loregrep", "scan", "--include", "*.rs"],
            vec!["loregrep", "scan", "--exclude", "target"],
            vec!["loregrep", "scan", "--follow-symlinks"],
        ] {
            assert!(
                Cli::try_parse_from(&flag).is_err(),
                "expected {flag:?} to be rejected"
            );
        }
        // The flag that is actually honoured still parses.
        assert!(Cli::try_parse_from(["loregrep", "scan", "--cache"]).is_ok());
    }

    #[test]
    fn the_directory_help_text_describes_what_the_flag_does() {
        // Documented behaviour must match implemented behaviour: the flag never
        // chdirs, so its help must not claim to.
        let cmd = Cli::command();
        let arg = cmd
            .get_arguments()
            .find(|a| a.get_id() == "directory")
            .expect("global --directory flag");
        let help = arg
            .get_help()
            .map(|h| h.to_string())
            .unwrap_or_else(|| arg.get_long_help().map(|h| h.to_string()).unwrap());
        assert!(help.contains("Relative paths"), "help was: {help}");
        assert!(help.contains("absolute"), "help was: {help}");
        assert!(
            help.contains("does not change the process working directory"),
            "help was: {help}"
        );
    }
}
