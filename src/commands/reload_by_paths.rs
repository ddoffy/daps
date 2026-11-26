use crate::helper::ParamStoreHelper;
use std::collections::HashMap;

/// Handles the `reload-by-paths <paths>` and `reloads` commands.
/// Re-fetches all parameters under a given path prefix from AWS SSM.
pub async fn reload_by_paths(
    helper: &mut ParamStoreHelper,
    paths: &str,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut all_values: HashMap<String, String> = HashMap::new();

    helper.completer.log(&format!("Reloading parameters by paths: {:?}", paths));
    // Accept both space-separated and comma-separated paths.
    let mut normalized_paths: Vec<String> = paths
        .split_whitespace()
        .flat_map(|chunk| chunk.split(','))
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(String::from)
        .collect();

    // If nothing was provided, reload everything under the configured base path.
    if normalized_paths.is_empty() {
        normalized_paths.push(helper.completer.base_path.clone());
    }

    for path in normalized_paths {
        let values = helper.completer.get_set_values(&path).await?;
        all_values.extend(values);
    }

    Ok(all_values)
}
