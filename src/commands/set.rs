use crate::helper::ParamStoreHelper;

/// Handles the `set <value>` command.
/// Sets the currently selected parameter to the given value in AWS SSM and updates the local cache.
pub async fn set_value(
    helper: &mut ParamStoreHelper,
    line: &str,
    path: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    println!("Setting parameter: {}", path);
    let value = line.replace("set", "");
    let value = value.trim_start().to_string();

    let value = helper.completer.change_value(path, value).await?;
    println!("Set value: {}", value);

    Ok(value)
}
