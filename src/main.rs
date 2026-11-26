use crate::cli::Subcommand;
use crate::command::Command;
use crate::completer::ParameterCompleter;
use crate::encryption::Encryption;
use crate::helper::ParamStoreHelper;
use crate::utils::parse_region;
use rustyline::{
    Cmd, CompletionType, Config, EditMode, Editor, EventHandler, KeyCode, KeyEvent, Modifiers,
    highlight::MatchingBracketHighlighter,
};
use structopt::StructOpt;

pub mod cli;
pub mod command;
pub mod commands;
pub mod complete;
pub mod completer;
pub mod encryption;
pub mod guide;
pub mod helper;
pub mod repl;
pub mod utils;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "daps",
    about = "D. AWS Parameter Store CLI",
    author = "D. Doffy <cuongnsm@gmail.com>"
)]
struct Opt {
    /// AWS Region
    #[structopt(long, default_value = "us-east-1")]
    region: String,

    /// Starting path for parameter store (e.g., /prod/)
    #[structopt(short, long, default_value = "/")]
    path: String,

    /// Refresh parameter cache on startup
    #[structopt(short, long)]
    refresh: bool,

    /// Store directory for parameters and values
    #[structopt(long, default_value = "parameters")]
    store_dir: String,

    /// Verbose output
    #[structopt(long)]
    verbose: bool,

    #[structopt(subcommand)]
    cmd: Option<Subcommand>,
}

/// Resolve `--store-dir`: absolute as given, otherwise relative to the home dir.
fn resolve_store_dir(store_dir: &str) -> String {
    #[cfg(not(target_os = "windows"))]
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    #[cfg(target_os = "windows")]
    let home_dir = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());

    let is_absolute = if cfg!(target_os = "windows") {
        store_dir.chars().nth(1) == Some(':')
    } else {
        store_dir.starts_with('/')
    };

    if is_absolute {
        store_dir.to_string()
    } else {
        format!("{}/{}", home_dir, store_dir)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── Completion helper: `daps __complete KIND WORD [STORE_DIR]` ─────────
    // Raw args so it stays out of --help and the generated scripts. Cache
    // files only: no AWS, and errors stay silent.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("__complete") {
        let arg = |i: usize, d: &str| args.get(i).map_or(d.to_string(), |s| s.clone());
        let key = std::env::var("DAPS_ENCRYPTION_KEY").unwrap_or_else(|_| "default_key".to_string());
        complete::run(&arg(2, ""), &arg(3, ""), &resolve_store_dir(&arg(4, "parameters")), &key);
        return Ok(());
    }

    let opt = Opt::from_args();

    // ── Shell completion generation (no AWS access needed) ─────────────────
    if let Some(Subcommand::Completion { shell }) = &opt.cmd {
        if matches!(shell, structopt::clap::Shell::Zsh) {
            let mut raw = Vec::new();
            Opt::clap().gen_completions_to("daps", *shell, &mut raw);
            print!("{}", complete::zsh_script(&String::from_utf8(raw)?));
        } else {
            Opt::clap().gen_completions_to("daps", *shell, &mut std::io::stdout());
        }
        return Ok(());
    }

    // ── Usage guide (no AWS access needed) ─────────────────────────────────
    if let Some(Subcommand::Guide { if_changed, if_new }) = &opt.cmd {
        guide::run(if_changed.clone(), *if_new);
        return Ok(());
    }

    let encryption_key = std::env::var("DAPS_ENCRYPTION_KEY").unwrap_or_else(|_| {
        if opt.verbose {
            eprintln!("DAPS_ENCRYPTION_KEY not set, using default");
        }
        "default_key".to_string()
    });
    let region = parse_region(&opt.region).map_err(|e| format!("Invalid region: {}", e))?;
    let base_path = opt.path.clone();

    if !base_path.starts_with('/') {
        return Err("Base path must start with '/'".into());
    }

    let store_dir = resolve_store_dir(&opt.store_dir);

    // ── Non-interactive CLI subcommands ────────────────────────────────────
    if let Some(sub) = opt.cmd {
        return cli::run(sub, region, base_path, opt.refresh, store_dir, opt.verbose, encryption_key).await;
    }

    // ── Interactive REPL mode (default when no subcommand given) ───────────
    let mut completer = ParameterCompleter::new(
        region,
        base_path,
        opt.refresh,
        store_dir,
        opt.verbose,
        Encryption::new(true, encryption_key),
    );
    completer.load_parameters().await?;

    let config = Config::builder()
        .edit_mode(EditMode::Vi)
        .completion_type(CompletionType::List)
        .auto_add_history(true)
        .bell_style(rustyline::config::BellStyle::None)
        .build();

    let mut rl: Editor<ParamStoreHelper> = Editor::with_config(config)?;
    rl.bind_sequence(
        KeyEvent(KeyCode::Tab, Modifiers::NONE),
        EventHandler::Simple(Cmd::Complete),
    );
    rl.set_helper(Some(ParamStoreHelper {
        completer,
        highlighter: MatchingBracketHighlighter::new(),
        commands: Command::keywords(),
    }));

    repl::run(&mut rl).await
}
