/// Every REPL command the user can type.
/// Parsing is centralised here so `main` only needs to `match` on clean variants.
#[derive(Debug)]
pub enum Command {
    Exit,
    Refresh,
    Migration,
    /// Re-fetches the currently-selected parameter from AWS.
    Reload,
    /// Prints the currently-selected parameter name.
    ShowSelected,
    /// Re-fetches all parameters under the selected path prefix (`reloads`).
    ReloadSelected,
    /// `reload-by-paths <paths>` — re-fetches all under an explicit prefix.
    ReloadByPaths(String),
    /// `reload-by-path <path>` — re-fetches one explicit parameter.
    ReloadByPath(String),
    /// `set <value>` — sets the currently-selected parameter to `value`.
    Set(String),
    /// `sel <index>` — picks a parameter from the last search result by index.
    SelectByIndex(String),
    /// `insert <path>:<value>:<type>` — creates a new parameter.
    Insert(String),
    /// `search <term>` — fuzzy-searches cached parameter keys.
    Search(String),
    /// Anything else is treated as a path to navigate / display.
    Navigate(String),
}

impl Command {
    /// Parses a raw REPL input line into a `Command`.
    pub fn parse(line: &str) -> Self {
        let line = line.trim();

        // Split into the leading keyword and everything after it.
        let (keyword, rest) = match line.find(' ') {
            Some(pos) => (&line[..pos], line[pos + 1..].trim()),
            None => (line, ""),
        };

        match keyword {
            "exit" => Command::Exit,
            "refresh" => Command::Refresh,
            "migration" => Command::Migration,
            "reload" => Command::Reload,
            "reloads" => Command::ReloadSelected,
            "reload-by-paths" => Command::ReloadByPaths(rest.to_string()),
            "reload-by-path" => Command::ReloadByPath(rest.to_string()),
            "set" => Command::Set(rest.to_string()),
            // "select" (no arg) → show current selection; "sel <n>" → pick by index
            "select" => Command::ShowSelected,
            "sel" => Command::SelectByIndex(rest.to_string()),
            "insert" => Command::Insert(rest.to_string()),
            "search" => Command::Search(rest.to_string()),
            _ => Command::Navigate(line.to_string()),
        }
    }

    /// Keyword strings exposed to the completer / highlighter.
    pub fn keywords() -> Vec<String> {
        vec![
            "exit",
            "refresh",
            "reload",
            "reloads",
            "set",
            "select",
            "sel",
            "reload-by-path",
            "reload-by-paths",
            "insert",
            "search",
            "migration",
        ]
        .into_iter()
        .map(String::from)
        .collect()
    }
}
