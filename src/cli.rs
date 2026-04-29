use crate::commands::insert::insert_value;
use crate::commands::migration::migration;
use crate::commands::parse_db::parse_db;
use crate::commands::refresh::refresh;
use crate::commands::reload::{reload, reload_by_path};
use crate::commands::reload_by_paths::reload_by_paths;
use crate::commands::search::search_cli;
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

    /// Run as an MCP (Model Context Protocol) server over stdio
    Mcp,
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
            let value = reload(&mut helper, &path).await?;
            println!("{}", value);
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
                use clipboard::{ClipboardContext, ClipboardProvider};
                let mut ctx = ClipboardContext::new()
                    .map_err(|e| format!("Failed to create clipboard context: {}", e))?;
                let mut cpboard = crate::cpboard::Cpboard::new(&mut ctx);
                parse_db(&path, &value, &mut cpboard);
            } else {
                // Piped: output `key=value` lines, no clipboard
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

        Subcommand::Mcp => unreachable!("Mcp handled in main"),
    }

    Ok(())
}
