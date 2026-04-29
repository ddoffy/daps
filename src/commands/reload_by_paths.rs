use crate::helper::ParamStoreHelper;
use std::collections::HashMap;

/// Handles the `reload-by-paths <paths>` and `reloads` commands.
/// Re-fetches all parameters under a given path prefix from AWS SSM.
pub async fn reload_by_paths(
    helper: &mut ParamStoreHelper,
    paths: &str,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    if paths.is_empty() {
        return Err("No paths provided for reload".into());
    }

    helper.completer.log(&format!("Reloading parameters by paths: {:?}", paths));
    let values = helper.completer.get_set_values(paths).await?;
    Ok(values)
}
