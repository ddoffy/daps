use crate::helper::ParamStoreHelper;

/// Handles the `reload` command: re-fetches the currently selected parameter from AWS SSM.
pub async fn reload(
    helper: &mut ParamStoreHelper,
    path: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    helper.completer.log(&format!("Reloading parameter: {}", path));
    let value = helper.completer.get_set_value(path).await?;
    helper.completer.log(&format!("Reloaded value: {}", value));
    Ok(value)
}

/// Handles the `reload-by-path <path>` command: re-fetches a specific parameter from AWS SSM.
pub async fn reload_by_path(
    helper: &mut ParamStoreHelper,
    path: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    helper.completer.log(&format!("Reloading parameter by path: {}", path));
    let value = helper.completer.get_set_value(path).await?;
    helper.completer.log(&format!("Reloaded value: {}", value));
    Ok(value)
}
