use crate::helper::ParamStoreHelper;
use colored::Colorize;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

/// Handles the `search <term>` command.
/// Performs fuzzy matching against all cached parameter keys and prints ranked results.
/// Stores matched keys into `helper.completer.search_result` for later use by `sel`.
pub fn search(helper: &mut ParamStoreHelper, search_term: &str) {
    let matcher = SkimMatcherV2::default();

    let mut matches: Vec<_> = helper
        .completer
        .values
        .keys()
        .filter_map(|k| matcher.fuzzy_match(k, search_term).map(|score| (k.clone(), score)))
        .collect();

    matches.sort_by(|a, b| b.1.cmp(&a.1));

    let keys: Vec<String> = matches.into_iter().take(20).map(|(key, _)| key).collect();

    if keys.is_empty() {
        // Fallback: simple contains search
        let fallback_keys: Vec<String> = helper
            .completer
            .values
            .keys()
            .filter(|k| k.to_lowercase().contains(&search_term.to_lowercase()))
            .cloned()
            .collect();

        if fallback_keys.is_empty() {
            println!("No matching parameters found for '{}'", search_term);
        } else {
            println!(
                "Fuzzy search found no matches, showing contains matches for '{}':",
                search_term
            );
            for (index, key) in fallback_keys.iter().enumerate() {
                let value = helper
                    .completer
                    .values
                    .get(key.as_str())
                    .map(|s| s.as_str())
                    .unwrap_or("<unavailable>");
                println!(
                    "{}: {} -> {}",
                    index.to_string().yellow(),
                    key,
                    value.red()
                );
            }
            helper.completer.search_result = fallback_keys;
        }
    } else {
        println!("Fuzzy search results for '{}':", search_term);
        for (index, key) in keys.iter().enumerate() {
            let value = helper
                .completer
                .values
                .get(key.as_str())
                .map(|s| s.as_str())
                .unwrap_or("<unavailable>");
            println!(
                "{}: {} -> {}",
                index.to_string().yellow(),
                key,
                value.red()
            );
        }
        helper.completer.search_result = keys;
    }
}
