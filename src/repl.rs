use crate::command::Command;
use crate::commands::handle_command_result;
use crate::commands::insert::insert_value;
use crate::commands::migration::migration;
use crate::commands::refresh::refresh;
use crate::commands::reload::{reload, reload_by_path};
use crate::commands::reload_by_paths::reload_by_paths;
use crate::commands::search::search;
use crate::commands::select::select_by_index;
use crate::commands::set::set_value;
use crate::cpboard::Cpboard;
use crate::helper::ParamStoreHelper;
use clipboard::ClipboardContext;
use colored::Colorize;
use rustyline::Editor;

/// Runs the interactive REPL loop.
///
/// Accepts the already-configured `Editor` (with helper attached) and a
/// `ClipboardContext`.  Returns when the user types `exit`, presses CTRL-C /
/// CTRL-D, or an unrecoverable readline error occurs.
pub async fn run(
    rl: &mut Editor<ParamStoreHelper>,
    ctx: &mut ClipboardContext,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("AWS Parameter Store CLI");
    println!(
        "Type a parameter path and use {} for completion",
        "Tab".red()
    );
    println!("Type '{}' to quit", "exit".yellow());

    let mut cpboard = Cpboard::new(ctx);
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
                                &mut cpboard,
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
                                &mut cpboard,
                            )
                            .await;
                        }
                    }

                    Command::Set(value) => {
                        if let Some(helper) = rl.helper_mut() {
                            handle_command_result(
                                set_value(helper, &value, &selected).await,
                                &mut cpboard,
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
                                &mut cpboard,
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

                    Command::Navigate(path) => {
                        rl.add_history_entry(&path);
                        selected = path.clone();

                        if let Some(helper) = rl.helper() {
                            if let Ok(mut metadata) = helper.completer.metadata.lock() {
                                metadata.insert("selected".to_string(), selected.clone());
                            }
                        }

                        if let Some(helper) = rl.helper() {
                            let mut matching_paths = Vec::new();
                            if let Ok(values) = helper.completer.values.lock() {
                                for key in values.keys() {
                                    if key.starts_with(&path) {
                                        matching_paths.push(key.clone());
                                    }
                                }
                            }

                            if let Ok(values) = helper.completer.values.lock() {
                                let mut clipboard_content = String::new();
                                for p in matching_paths {
                                    if let Some(value) = values.get(&p) {
                                        println!(
                                            "Found value for {}: {}",
                                            p.green(),
                                            value.red()
                                        );
                                        clipboard_content
                                            .push_str(&format!("{}: {}\n", p, value));
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
