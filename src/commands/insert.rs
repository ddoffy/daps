use crate::helper::ParamStoreHelper;

/// Handles the `insert <path>:<value>:<type>` command.
/// `raw` is the already-parsed argument (everything after "insert "), format: `/path:value:Type`.
/// Creates a new parameter in AWS SSM and adds it to the local cache.
pub async fn insert_value(
    helper: &mut ParamStoreHelper,
    raw: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    helper.completer.log(&format!("Inserting parameter: {}", raw));
    let path_and_value = raw.to_string();

    // Format: /path/to/parameter:value:Type
    let index = path_and_value.find(':').ok_or("Invalid format")?;
    let last_index = path_and_value.rfind(':').ok_or("Invalid format")?;

    let param_type = if last_index != index {
        Some(path_and_value[last_index + 1..].to_string())
    } else {
        None
    };

    let path = &path_and_value[..index];
    let value = &path_and_value[index + 1..last_index];

    helper
        .completer
        .set_parameter(path, value.to_string(), param_type)
        .await?;
    helper
        .completer
        .update_all(path, value.to_string())
        .await?;

    helper.completer.log(&format!("Inserted value: {}", value));
    Ok(value.to_string())
}
