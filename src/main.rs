use crate::cpboard::Cpboard;
use crate::commands::handle_command_result;
use crate::commands::insert::insert_value;
use crate::commands::migration::migration;
use crate::commands::refresh::refresh;
use crate::commands::reload::{reload, reload_by_path};
use crate::commands::reload_by_paths::reload_by_paths;
use crate::commands::search::search;
use crate::commands::select::select_by_index;
use crate::commands::set::set_value;
use crate::completer::ParameterCompleter;
use crate::encryption::Encryption;
use crate::helper::ParamStoreHelper;
use crate::utils::parse_region;
use clipboard::ClipboardProvider;
use clipboard::ClipboardContext;
use colored::Colorize;
use rustyline::{
    CompletionType, Config, EditMode, Editor,
    highlight::MatchingBracketHighlighter,
};
use structopt::StructOpt;

pub mod cpboard;
pub mod commands;
pub mod completer;
pub mod encryption;
pub mod helper;
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

    let encryption = Encryption::new(true, encryption_key);
    let opt = Opt::from_args();
    let region = parse_region(&opt.region).map_err(|e| format!("Invalid region: {}", e))?;
    let base_path = opt.path.clone();

    if !base_path.starts_with('/') {
        return Err("Base path must start with '/'".into());
    }

    #[cfg(not(target_os = "windows"))]
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| {
        println!("HOME environment variable not set, using current directory");
        ".".to_string()
    });

    #[cfg(target_os = "windows")]
    let home_dir = std::env::var("APPDATA").unwrap_or_else(|_| {
        println!("APPDATA environment variable not set, using current directory");
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

    let completer = ParameterCompleter::new(
        region,
        base_path,
        opt.refresh,
        store_dir,
        opt.verbose,
        encryption,
    );

    completer.load_parameters().await?;

    let helper = ParamStoreHelper {
        completer,
        highlighter: MatchingBracketHighlighter::new(),
        commands: vec![
            "exit".to_string(),
            "refresh".to_string(),
            "reload".to_string(),
            "reloads".to_string(),
            "set".to_string(),
            "select".to_string(),
            "sel".to_string(),
            "reload-by-path".to_string(),
            "reload-by-paths".to_string(),
            "insert".to_string(),
            "search".to_string(),
            "migration".to_string(),
        ],
    };

    let config = Config::builder()
        .edit_mode(EditMode::Vi)
        .completion_type(CompletionType::Circular)
        .auto_add_history(true)
        .bell_style(rustyline::config::BellStyle::None)
        .build();

    let mut rl = Editor::with_config(config)?;
    rl.set_helper(Some(helper));

    println!("AWS Parameter Store CLI");
    println!(
        "Type a parameter path and use {} for completion",
        "Tab".red()
    );
    println!("Type '{}' to quit", "exit".yellow());

    let mut ctx = ClipboardContext::new()
        .map_err(|e| format!("Failed to create clipboard context: {}", e))?;
    let mut cpboard = Cpboard::new(&mut ctx);

    let mut selected = String::new();

    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                match line.as_str() {
                    "exit" => break,

                    "refresh" => {
                        if let Some(helper) = rl.helper_mut() {
                            if let Err(err) = refresh(helper).await {
                                println!("Error refreshing parameters: {}", err);
                            }
                        }
                        continue;
                    }

                    "migration" => {
                        if let Some(helper) = rl.helper_mut() {
                            if let Err(err) = migration(helper).await {
                                println!("Error during migration: {}", err);
                            }
                        }
                        continue;
                    }

                    "reload" => {
                        if let Some(helper) = rl.helper_mut() {
                            handle_command_result(
                                reload(helper, &selected).await,
                                &mut cpboard,
                            )
                            .await;
                        }
                        continue;
                    }

                    "select" => {
                        if !selected.is_empty() {
                            println!(
                                "Currently selected parameter: {}",
                                selected.green()
                            );
                        } else {
                            println!(
                                "No parameter selected. Use 'sel <index>' to select one."
                            );
                        }
                        continue;
                    }

                    "reloads" => {
                        if let Some(helper) = rl.helper_mut() {
                            let paths = if selected.is_empty() {
                                println!(
                                    "No parameter selected. Reloading all parameters."
                                );
                                ""
                            } else {
                                &selected
                            };
                            reload_by_paths(helper, paths).await?;
                        }
                        continue;
                    }

                    cmd if cmd.starts_with("reload-by-paths") => {
                        if let Some(helper) = rl.helper_mut() {
                            let paths = line.replace("reload-by-paths", "");
                            let paths = paths.trim().to_string();
                            let paths = if paths.is_empty() {
                                println!("No paths provided, using selected.");
                                selected.clone()
                            } else {
                                paths
                            };
                            reload_by_paths(helper, &paths).await?;
                        }
                        continue;
                    }

                    cmd if cmd.starts_with("reload-by-path") => {
                        if let Some(helper) = rl.helper_mut() {
                            let path = line.replace("reload-by-path", "");
                            let path = path.trim().to_string();
                            let path = if path.is_empty() {
                                println!("No path provided, using selected.");
                                selected.clone()
                            } else {
                                path
                            };
                            handle_command_result(
                                reload_by_path(helper, &path).await,
                                &mut cpboard,
                            )
                            .await;
                        }
                        continue;
                    }

                    cmd if cmd.starts_with("set") => {
                        if let Some(helper) = rl.helper_mut() {
                            handle_command_result(
                                set_value(helper, &line, &selected).await,
                                &mut cpboard,
                            )
                            .await;
                        }
                        continue;
                    }

                    cmd if cmd.starts_with("sel") => {
                        if let Some(helper) = rl.helper_mut() {
                            let arg = line
                                .replace(line.split(' ').next().unwrap_or_default(), "")
                                .trim()
                                .to_string();

                            match select_by_index(helper, &arg) {
                                Ok(param) => selected = param,
                                Err(err) => println!("{}", err),
                            }
                        }
                        continue;
                    }

                    cmd if cmd.starts_with("insert") => {
                        if let Some(helper) = rl.helper_mut() {
                            handle_command_result(
                                insert_value(helper, &line).await,
                                &mut cpboard,
                            )
                            .await;
                        }
                        continue;
                    }

                    cmd if cmd.starts_with("search") => {
                        if let Some(helper) = rl.helper_mut() {
                            let search_term = line.replace("search", "");
                            let search_term = search_term.trim().to_string();

                            if search_term.is_empty() {
                                println!(
                                    "Please provide a search term. Usage: search <term>"
                                );
                                continue;
                            }

                            search(helper, &search_term);
                        }
                        continue;
                    }

                    _ => {}
                }

                rl.add_history_entry(line.as_str());
                selected = line.clone();

                if let Some(helper) = rl.helper() {
                    if let Ok(mut metadata) = helper.completer.metadata.lock() {
                        metadata.insert("selected".to_string(), selected.clone());
                    }
                }

                // Print all matching parameter values and copy them to clipboard
                if let Some(helper) = rl.helper() {
                    let mut paths = Vec::new();
                    if let Ok(values) = helper.completer.values.lock() {
                        for key in values.keys() {
                            if key.starts_with(&line) {
                                paths.push(key.clone());
                            }
                        }
                    }

                    if let Ok(values) = helper.completer.values.lock() {
                        let mut clipboard_content = String::new();
                        for path in paths {
                            if let Some(value) = values.get(&path) {
                                println!(
                                    "Found value for {}: {}",
                                    path.green(),
                                    value.red()
                                );
                                clipboard_content
                                    .push_str(&format!("{}: {}\n", path, value));
                            }
                        }
                        if let Err(err) =
                            cpboard.set_clipboard_content(&clipboard_content)
                        {
                            println!("Error copying to clipboard: {}", err);
                        } else {
                            println!("Copied to clipboard:\n{}", clipboard_content);
                        }
                    }
                }
            }

            Err(rustyline::error::ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }

    Ok(())
}
