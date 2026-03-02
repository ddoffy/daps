use crate::command::Command;
use crate::completer::ParameterCompleter;
use crate::encryption::Encryption;
use crate::helper::ParamStoreHelper;
use crate::utils::parse_region;
use clipboard::ClipboardContext;
use clipboard::ClipboardProvider;
use rustyline::{
    CompletionType, Config, EditMode, Editor,
    highlight::MatchingBracketHighlighter,
};
use structopt::StructOpt;

pub mod command;
pub mod commands;
pub mod completer;
pub mod cpboard;
pub mod encryption;
pub mod helper;
pub mod repl;
pub mod utils;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "daps",
    about = "D. AWS Parameter Store CLI with tab completion",
    author = "D. Doffy <cuongnsm@gmail.com>"
)]
struct Opt {
    /// AWS Region
    #[structopt(long, default_value = "us-east-1")]
    region: String,

    /// Starting path for parameter store (e.g., /prod/)
    #[structopt(short, long, default_value = "/")]
    path: String,

    /// Refresh parameter cache
    #[structopt(short, long)]
    refresh: bool,

    /// Store directory for parameters and values
    #[structopt(long, default_value = "parameters")]
    store_dir: String,

    /// Verbose output
    #[structopt(long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let encryption_key = std::env::var("DAPS_ENCRYPTION_KEY").unwrap_or_else(|_| {
        println!("DAPS_ENCRYPTION_KEY not set, using default");
        "default_key".to_string()
    });

    let opt = Opt::from_args();
    let region = parse_region(&opt.region).map_err(|e| format!("Invalid region: {}", e))?;
    let base_path = opt.path.clone();

    if !base_path.starts_with('/') {
        return Err("Base path must start with '/'".into());
    }

    #[cfg(not(target_os = "windows"))]
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| {
        println!("HOME not set, using current directory");
        ".".to_string()
    });

    #[cfg(target_os = "windows")]
    let home_dir = std::env::var("APPDATA").unwrap_or_else(|_| {
        println!("APPDATA not set, using current directory");
        ".".to_string()
    });

    let is_absolute = if cfg!(target_os = "windows") {
        opt.store_dir.chars().nth(1) == Some(':')
    } else {
        opt.store_dir.starts_with('/')
    };

    let store_dir = if is_absolute {
        opt.store_dir.clone()
    } else {
        format!("{}/{}", home_dir, opt.store_dir)
    };

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
        .completion_type(CompletionType::Circular)
        .auto_add_history(true)
        .bell_style(rustyline::config::BellStyle::None)
        .build();

    let mut rl: Editor<ParamStoreHelper> = Editor::with_config(config)?;
    rl.set_helper(Some(ParamStoreHelper {
        completer,
        highlighter: MatchingBracketHighlighter::new(),
        commands: Command::keywords(),
    }));

    let mut ctx = ClipboardContext::new()
        .map_err(|e| format!("Failed to create clipboard context: {}", e))?;

    repl::run(&mut rl, &mut ctx).await
}
