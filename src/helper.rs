use crate::completer::ParameterCompleter;
use rustyline::{
    Context,
    completion::{Completer, Pair},
    error::ReadlineError,
    highlight::{Highlighter, MatchingBracketHighlighter},
    hint::{Hint, Hinter},
    validate::Validator,
    Helper,
};
use std::borrow::Cow::{self, Borrowed, Owned};

pub struct ParamStoreHelper {
    pub completer: ParameterCompleter,
    pub highlighter: MatchingBracketHighlighter,
    pub commands: Vec<String>,
}

impl Completer for ParamStoreHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        let path = line[..pos].trim();
        let start = 0;

        let completions = self.completer.get_completions(path);

        let mut candidates: Vec<Pair> = completions
            .into_iter()
            .map(|s| Pair {
                display: s.clone(),
                replacement: s,
            })
            .collect();

        let cmd_candidates: Vec<Pair> = self
            .commands
            .iter()
            .filter(|cmd| cmd.to_lowercase().starts_with(&path.to_lowercase()))
            .map(|s| Pair {
                display: s.clone(),
                replacement: s.clone(),
            })
            .collect();

        candidates.extend(cmd_candidates);
        Ok((start, candidates))
    }
}

impl Highlighter for ParamStoreHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        use colored::*;

        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        let command = parts.first().unwrap_or(&"");
        let args = parts.get(1).unwrap_or(&"");

        if self.commands.contains(&command.to_lowercase()) {
            Owned(format!("{} {}", command.blue(), args))
        } else {
            Borrowed(line)
        }
    }

    fn highlight_char(&self, _line: &str, _pos: usize) -> bool {
        self.highlighter.highlight_char(_line, _pos)
    }
}

pub struct EmptyHint;

impl Hint for EmptyHint {
    fn display(&self) -> &str {
        ""
    }

    fn completion(&self) -> Option<&str> {
        Some("")
    }
}

impl Hinter for ParamStoreHelper {
    type Hint = EmptyHint;

    fn hint(&self, _line: &str, _pos: usize, _ctx: &Context<'_>) -> Option<Self::Hint> {
        None
    }
}

impl Validator for ParamStoreHelper {}

impl Helper for ParamStoreHelper {}

impl std::fmt::Debug for ParamStoreHelper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ParamStoreHelper")
    }
}
