use crate::command::Command;
use crate::commands::handle_command_result;
use crate::commands::insert::insert_value;
use crate::commands::migration::migration;
use crate::commands::refresh::refresh;
use crate::commands::reload::{reload, reload_by_path};
use crate::commands::reload_by_paths::reload_by_paths;
use crate::commands::parse_db::parse_db;
use crate::commands::search::search;
use crate::commands::select::select_by_index;
use crate::commands::set::set_value;
use crate::helper::ParamStoreHelper;
use colored::Colorize;
use rustyline::Editor;

/// Runs the interactive REPL loop.
///
/// Accepts the already-configured `Editor` (with helper attached).
/// Returns when the user types `exit`, presses CTRL-C / CTRL-D,
/// or an unrecoverable readline error occurs.
pub async fn run(
    rl: &mut Editor<ParamStoreHelper>,
) -> Result<(), Box<dyn std::error::Error>> {
    let base_path = rl
        .helper()
        .map(|h| h.completer.base_path.clone())
        .unwrap_or_default();
    let param_count = rl
        .helper()
        .map(|h| h.completer.values.len())
        .unwrap_or(0);

    println!("AWS Parameter Store CLI");
    println!(
        "Base path: {}  |  {} parameters cached",
        base_path.cyan(),
        param_count.to_string().yellow()
    );
    println!(
        "Type a parameter path and use {} for completion",
        "Tab".red()
    );
    println!("Type '{}' to quit", "exit".yellow());

    let mut selected = String::new();

    loop {
        match rl.readline(">> ") {
            Ok(line) => {
                match Command::parse(&line) {
                    Command::Exit => break,

                    Command::Refresh => {
                        if let Some(helper) = rl.helper_mut() {
                            if let Err(err) = refresh(helper).await {
                                println!("Error refreshing parameters: {}", err);
                            }
                        }
                    }

                    Command::Migration => {
                        if let Some(helper) = rl.helper_mut() {
                            if let Err(err) = migration(helper).await {
                                println!("Error during migration: {}", err);
                            }
                        }
                    }

                    Command::Reload => {
                        if let Some(helper) = rl.helper_mut() {
                            handle_command_result(
                                reload(helper, &selected).await,
                            )
                            .await;
                        }
                    }

                    Command::ShowSelected => {
                        if !selected.is_empty() {
                            println!("Currently selected parameter: {}", selected.green());
                        } else {
                            println!(
                                "No parameter selected. Use 'sel <index>' to select one."
                            );
                        }
                    }

                    Command::ReloadSelected => {
                        if let Some(helper) = rl.helper_mut() {
                            let paths = if selected.is_empty() {
                                println!("No parameter selected. Reloading all parameters.");
                                String::new()
                            } else {
                                selected.clone()
                            };
                            reload_by_paths(helper, &paths).await?;
                        }
                    }

                    Command::ReloadByPaths(paths) => {
                        if let Some(helper) = rl.helper_mut() {
                            let paths = if paths.is_empty() {
                                println!("No paths provided, using selected.");
                                selected.clone()
                            } else {
                                paths
                            };
                            reload_by_paths(helper, &paths).await?;
                        }
                    }

                    Command::ReloadByPath(path) => {
                        if let Some(helper) = rl.helper_mut() {
                            let path = if path.is_empty() {
                                println!("No path provided, using selected.");
                                selected.clone()
                            } else {
                                path
                            };
                            handle_command_result(
                                reload_by_path(helper, &path).await,
                            )
                            .await;
                        }
                    }

                    Command::Set(value) => {
                        if let Some(helper) = rl.helper_mut() {
                            handle_command_result(
                                set_value(helper, &value, &selected).await,
                            )
                            .await;
                        }
                    }

                    Command::SelectByIndex(arg) => {
                        if let Some(helper) = rl.helper_mut() {
                            match select_by_index(helper, &arg) {
                                Ok(param) => selected = param,
                                Err(err) => println!("{}", err),
                            }
                        }
                    }

                    Command::Insert(raw) => {
                        if let Some(helper) = rl.helper_mut() {
                            handle_command_result(
                                insert_value(helper, &raw).await,
                            )
                            .await;
                        }
                    }

                    Command::Search(term) => {
                        if term.is_empty() {
                            println!("Please provide a search term. Usage: search <term>");
                        } else if let Some(helper) = rl.helper_mut() {
                            search(helper, &term);
                        }
                    }

                    Command::ParseDb => {
                        if selected.is_empty() {
                            println!("No parameter selected. Use 'sel <index>' or navigate to a key first.");
                        } else if let Some(helper) = rl.helper() {
                            let value = helper.completer.values.get(&selected).cloned();
                            match value {
                                Some(conn_str) => parse_db(&selected, &conn_str),
                                None => println!("No cached value for '{}'. Try 'reload' first.", selected),
                            }
                        }
                    }

                    Command::Navigate(path) => {
                        rl.add_history_entry(&path);
                        selected = path.clone();

                        if let Some(helper) = rl.helper_mut() {
                            helper
                                .completer
                                .metadata
                                .insert("selected".to_string(), selected.clone());

                            let matching_paths: Vec<String> = helper
                                .completer
                                .values
                                .keys()
                                .filter(|k| k.starts_with(&path))
                                .cloned()
                                .collect();

                            for p in matching_paths {
                                if let Some(value) = helper.completer.values.get(&p) {
                                    println!(
                                        "Found value for {}: {}",
                                        p.green(),
                                        value.red()
                                    );
                                }
                            }
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
