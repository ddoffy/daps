use rusoto_core::Region;
use std::fs;
use std::io::{self, BufRead, BufReader, Seek, SeekFrom, Write};

pub fn parse_region(region: &str) -> Result<Region, String> {
    match region
        .parse::<Region>()
        .map_err(|_| format!("Invalid region: {}", region))
    {
        Ok(region) => Ok(region),
        Err(err) => Err(format!("Error parsing region: {}", err)),
    }
}

/// Replaces the first line matching a predicate and exits immediately.
pub fn replace_first_matching_line(
    filepath: &str,
    line_matcher: impl Fn(&str) -> bool,
    replacement_line: &str,
) -> io::Result<bool> {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(filepath)?;

    let mut reader = BufReader::new(&file);
    let mut current_pos: u64 = 0;
    let mut found_match = false;
    let mut line = String::new();

    while reader.read_line(&mut line)? > 0 {
        if !found_match && line_matcher(&line) {
            found_match = true;

            let mut file = reader.into_inner();
            file.seek(SeekFrom::Start(current_pos))?;

            let mut replacement = replacement_line.to_string();
            if !replacement.ends_with('\n') {
                replacement.push('\n');
            }

            file.write_all(replacement.as_bytes())?;

            if replacement.len() < line.len() {
                let padding = " ".repeat(line.len() - replacement.len());
                file.write_all(padding.as_bytes())?;
            }
            break;
        }
        current_pos += line.len() as u64;
        line.clear();
    }

    Ok(found_match)
}

/// Convenience wrapper: replaces the first line containing `search_text`.
pub fn replace_first_line_containing(
    filepath: &str,
    search_text: &str,
    replacement_line: &str,
) -> io::Result<bool> {
    replace_first_matching_line(filepath, |line| line.contains(search_text), replacement_line)
}
