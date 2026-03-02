use crate::cpboard::Cpboard;
use colored::Colorize;

/// Parses a Postgres-style connection string (`key=value;key=value;...`) found
/// in the selected SSM parameter's value, prints the fields, and copies them
/// to the clipboard.
///
/// `param_key`  – the SSM parameter path (shown in the header)
/// `raw`        – the raw connection string value from the cache
pub fn parse_db<'a>(param_key: &str, raw: &str, cpboard: &mut Cpboard<'a>) {
    let raw = raw.trim().trim_matches(|c| c == '"' || c == '\'');

    if raw.is_empty() {
        println!("Value of '{}' is empty — nothing to parse.", param_key);
        return;
    }

    // Split on ';', tolerate trailing semicolons and extra whitespace
    let pairs: Vec<(&str, &str)> = raw
        .split(';')
        .filter_map(|segment| {
            let segment = segment.trim();
            if segment.is_empty() {
                return None;
            }
            // Split on the first '=' only so values may contain '='
            let eq = segment.find('=')?;
            let key = segment[..eq].trim();
            let value = segment[eq + 1..].trim().trim_matches(|c| c == '"' || c == '\'');
            Some((key, value))
        })
        .collect();

    if pairs.is_empty() {
        println!(
            "Could not parse any key=value pairs from the value of '{}'.",
            param_key
        );
        return;
    }

    println!(
        "{} {}",
        "── DB connection from:".dimmed(),
        param_key.cyan()
    );

    let mut clipboard_content = String::new();
    for (key, value) in &pairs {
        println!("  {:<12} {}", format!("{}:", key).cyan().bold(), value.yellow());
        clipboard_content.push_str(&format!("{}: {}\n", key, value));
    }

    println!("{}", "─────────────────────────────────────────────".dimmed());

    match cpboard.set_clipboard_content(&clipboard_content) {
        Ok(_) => println!("{}", "✓ Copied to clipboard".green()),
        Err(err) => println!("Error copying to clipboard: {}", err),
    }
}
