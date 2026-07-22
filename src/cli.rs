use crate::commands::insert::insert_value;
use crate::commands::migration::migration;
use crate::commands::parse_db::parse_db;
use crate::commands::refresh::refresh;
use crate::commands::reload::{reload, reload_by_path};
use crate::commands::reload_by_paths::reload_by_paths;
use crate::commands::search::search_cli;
use crate::commands::secret_get::secret_get;
use crate::commands::secret_list::secret_list;
use crate::commands::secret_set::secret_set;
use crate::commands::set::set_value;
use crate::completer::ParameterCompleter;
use crate::encryption::Encryption;
use crate::helper::ParamStoreHelper;
use crate::command::Command;
use is_terminal::IsTerminal;
use rusoto_core::Region;
use rustyline::highlight::MatchingBracketHighlighter;
use std::io::{self, Read};

#[derive(Debug, structopt::StructOpt)]
pub enum Subcommand {
    /// Get a parameter value from AWS SSM (outputs raw value to stdout)
    Get {
        /// Parameter path (e.g. /prod/db/password)
        path: String,
    },

    /// Get all parameter values under a path prefix from AWS SSM (tab-separated key\tvalue lines)
    Gets {
        /// Path prefix (e.g. /prod/)
        path: String,
        /// Output keys only (one per line)
        #[structopt(long)]
        keys_only: bool,
    },

    /// Set a parameter value in AWS SSM
    ///
    /// The value can be piped from stdin: echo "newval" | daps set /prod/key
    Set {
        /// Parameter path
        path: String,
        /// New value (reads from stdin if omitted and stdin is a pipe)
        value: Option<String>,
    },

    /// Insert a new parameter into AWS SSM
    Insert {
        /// Parameter path
        path: String,
        /// Parameter value
        value: String,
        /// Parameter type: String | StringList | SecureString
        #[structopt(short = "t", long = "type", default_value = "String")]
        param_type: String,
    },

    /// Fuzzy-search cached parameter keys
    Search {
        /// Search term
        term: String,
        /// Output keys only (one per line), suitable for piping
        #[structopt(long)]
        keys_only: bool,
    },

    /// Re-fetch a parameter from AWS SSM and update the local cache
    Reload {
        /// Parameter path
        path: String,
    },

    /// Re-fetch all parameters under one or more path prefixes
    ReloadPaths {
        /// Path prefixes to reload (e.g. /prod/ /staging/)
        #[structopt(required = true)]
        paths: Vec<String>,
    },

    /// Refresh the entire local parameter cache from AWS SSM
    Refresh,

    /// Re-encrypt the local cache with the current DAPS_ENCRYPTION_KEY
    Migrate,

    /// Parse a Postgres connection string stored in a parameter
    ParseDb {
        /// Parameter path whose value is a connection string
        path: String,
    },

    /// Fetch a secret value from AWS Secrets Manager
    SecretGet {
        /// Secret name or ARN
        name: String,
    },

    /// Update an existing secret's value in AWS Secrets Manager
    SecretSet {
        /// Secret name or ARN
        name: String,
        /// New secret value (reads from stdin if omitted and stdin is a pipe)
        value: Option<String>,
    },

    /// Create a new secret in AWS Secrets Manager
    SecretCreate {
        /// Secret name
        name: String,
        /// Secret value
        value: String,
        /// Optional description
        #[structopt(short = "d", long)]
        description: Option<String>,
    },

    /// List secrets in AWS Secrets Manager
    SecretList {
        /// Optional name filter substring
        filter: Option<String>,
    },

    /// Generate a shell completion script and print it to stdout
    ///
    /// Examples:
    ///   daps completion bash > /etc/bash_completion.d/daps
    ///   daps completion zsh  > ~/.zfunc/_daps
    ///   daps completion fish > ~/.config/fish/completions/daps.fish
    Completion {
        /// Target shell
        #[structopt(possible_values = &["bash", "zsh", "fish"])]
        shell: structopt::clap::Shell,
    },
}

/// Builds a lightweight `ParamStoreHelper` for CLI use.
/// Only loads cached parameters when `need_cache` is true (e.g. search).
async fn make_helper(
    region: Region,
    base_path: String,
    refresh_cache: bool,
    store_dir: String,
    verbose: bool,
    encryption_key: String,
    need_cache: bool,
) -> Result<ParamStoreHelper, Box<dyn std::error::Error>> {
    let mut completer = ParameterCompleter::new(
        region,
        base_path,
        refresh_cache,
        store_dir,
        verbose,
        Encryption::new(true, encryption_key),
    );
    if need_cache {
        completer.load_parameters().await?;
    }
    Ok(ParamStoreHelper {
        completer,
        highlighter: MatchingBracketHighlighter::new(),
        commands: Command::keywords(),
    })
}

/// Read a value from stdin (used when `--value` is omitted and stdin is a pipe).
fn read_stdin_value() -> Result<String, Box<dyn std::error::Error>> {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    Ok(buf.trim_end_matches('\n').to_string())
}

pub async fn run(
    sub: Subcommand,
    region: Region,
    base_path: String,
    refresh_cache: bool,
    store_dir: String,
    verbose: bool,
    encryption_key: String,
) -> Result<(), Box<dyn std::error::Error>> {
    // Suppress colors when stdout is not a TTY (piping).
    let use_color = std::io::stdout().is_terminal();
    if !use_color {
        colored::control::set_override(false);
    }

    match sub {
        // ── get ────────────────────────────────────────────────────────────
        Subcommand::Get { path } => {
            let mut helper = make_helper(
                region, base_path, refresh_cache, store_dir, verbose, encryption_key, false,
            ).await?;
            let value = helper.completer.get_set_value(&path).await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            println!("{}", value);
        }

        // ── gets ───────────────────────────────────────────────────────────
        Subcommand::Gets { path, keys_only } => {
            let mut helper = make_helper(
                region, base_path, refresh_cache, store_dir, verbose, encryption_key, false,
            ).await?;
            let values = helper.completer.get_set_values(&path).await?;
            let mut pairs: Vec<_> = values.into_iter().collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            for (key, value) in &pairs {
                if keys_only {
                    println!("{}", key);
                } else {
                    println!("{}\t{}", key, value);
                }
            }
        }

        // ── set ────────────────────────────────────────────────────────────
        Subcommand::Set { path, value } => {
            let v = match value {
                Some(v) => v,
                None => {
                    if io::stdin().is_terminal() {
                        return Err("No value provided. Pass a value argument or pipe it via stdin.".into());
                    }
                    read_stdin_value()?
                }
            };
            let mut helper = make_helper(
                region, base_path, refresh_cache, store_dir, verbose, encryption_key, false,
            ).await?;
            let result = set_value(&mut helper, &v, &path).await?;
            println!("{}", result);
        }

        // ── insert ─────────────────────────────────────────────────────────
        Subcommand::Insert { path, value, param_type } => {
            let raw = format!("{}:{}:{}", path, value, param_type);
            let mut helper = make_helper(
                region, base_path, refresh_cache, store_dir, verbose, encryption_key, false,
            ).await?;
            let result = insert_value(&mut helper, &raw).await?;
            println!("{}", result);
        }

        // ── search ─────────────────────────────────────────────────────────
        Subcommand::Search { term, keys_only } => {
            let mut helper = make_helper(
                region, base_path, refresh_cache, store_dir, verbose, encryption_key, true,
            ).await?;
            let results = search_cli(&mut helper, &term);
            if keys_only {
                for key in &results {
                    println!("{}", key);
                }
            } else {
                for key in &results {
                    let value = helper
                        .completer
                        .values
                        .get(key.as_str())
                        .map(|s| s.as_str())
                        .unwrap_or("<unavailable>");
                    println!("{}\t{}", key, value);
                }
            }
        }

        // ── reload ─────────────────────────────────────────────────────────
        Subcommand::Reload { path } => {
            let mut helper = make_helper(
                region, base_path, refresh_cache, store_dir, verbose, encryption_key, false,
            ).await?;
            let value = reload_by_path(&mut helper, &path).await?;
            println!("{}", value);
        }

        // ── reload-paths ───────────────────────────────────────────────────
        Subcommand::ReloadPaths { paths } => {
            let joined = paths.join(" ");
            let mut helper = make_helper(
                region, base_path, refresh_cache, store_dir, verbose, encryption_key, false,
            ).await?;
            let values = reload_by_paths(&mut helper, &joined).await?;
            for (key, value) in &values {
                println!("{}\t{}", key, value);
            }
        }

        // ── refresh ────────────────────────────────────────────────────────
        Subcommand::Refresh => {
            let mut helper = make_helper(
                region, base_path, true, store_dir, verbose, encryption_key, true,
            ).await?;
            refresh(&mut helper).await?;
            eprintln!("Cache refreshed.");
        }

        // ── migrate ────────────────────────────────────────────────────────
        Subcommand::Migrate => {
            let mut helper = make_helper(
                region, base_path, refresh_cache, store_dir, verbose, encryption_key, true,
            ).await?;
            migration(&mut helper).await?;
            eprintln!("Migration complete.");
        }

        // ── parse-db ───────────────────────────────────────────────────────
        Subcommand::ParseDb { path } => {
            let mut helper = make_helper(
                region, base_path, refresh_cache, store_dir, verbose, encryption_key, false,
            ).await?;
            let value = reload(&mut helper, &path).await?;
            if use_color {
                parse_db(&path, &value);
            } else {
                let raw = value.trim().trim_matches(|c| c == '"' || c == '\'');
                for segment in raw.split(';') {
                    let segment = segment.trim();
                    if segment.is_empty() {
                        continue;
                    }
                    if let Some(eq) = segment.find('=') {
                        let k = segment[..eq].trim();
                        let v = segment[eq + 1..].trim().trim_matches(|c| c == '"' || c == '\'');
                        println!("{}={}", k, v);
                    }
                }
            }
        }

        // ── secret-get ─────────────────────────────────────────────────────
        Subcommand::SecretGet { name } => {
            let mut helper = make_helper(
                region, base_path, refresh_cache, store_dir, verbose, encryption_key, false,
            ).await?;
            let value = secret_get(&mut helper, &name).await?;
            println!("{}", value);
        }

        // ── secret-set ─────────────────────────────────────────────────────
        Subcommand::SecretSet { name, value } => {
            let v = match value {
                Some(v) => v,
                None => {
                    if io::stdin().is_terminal() {
                        return Err("No value provided. Pass a value argument or pipe via stdin.".into());
                    }
                    read_stdin_value()?
                }
            };
            let mut helper = make_helper(
                region, base_path, refresh_cache, store_dir, verbose, encryption_key, false,
            ).await?;
            let msg = secret_set(&mut helper, &name, &v).await?;
            eprintln!("{}", msg);
        }

        // ── secret-create ──────────────────────────────────────────────────
        Subcommand::SecretCreate { name, value, description } => {
            let mut helper = make_helper(
                region, base_path, refresh_cache, store_dir, verbose, encryption_key, false,
            ).await?;
            helper.completer.create_secret(&name, &value, description).await?;
            eprintln!("Secret '{}' created.", name);
        }

        // ── secret-list ────────────────────────────────────────────────────
        Subcommand::SecretList { filter } => {
            let helper = make_helper(
                region, base_path, refresh_cache, store_dir, verbose, encryption_key, false,
            ).await?;
            let output = secret_list(&helper, filter.as_deref()).await?;
            println!("{}", output);
        }

        // ── completion ─────────────────────────────────────────────────────
        // Handled in main before AWS setup; never reaches here.
        Subcommand::Completion { .. } => unreachable!("completion is handled in main"),
    }

    Ok(())
}