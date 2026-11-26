use crate::helper::ParamStoreHelper;

/// Handles the `set <value>` command.
/// `value` is the already-parsed argument (everything after "set ").
/// Sets the currently selected parameter to the given value in AWS SSM and updates the local cache.
pub async fn set_value(
    helper: &mut ParamStoreHelper,
    value: &str,
    path: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    helper.completer.log(&format!("Setting parameter: {}", path));
    let value = helper.completer.change_value(path, value.to_string()).await?;
    helper.completer.log(&format!("Set value: {}", value));
    Ok(value)
}
