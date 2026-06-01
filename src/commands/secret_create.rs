use crate::helper::ParamStoreHelper;

/// Parse `raw` as `name:value` or `name:value:description` and create the secret.
pub async fn secret_create(
    helper: &mut ParamStoreHelper,
    raw: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let parts: Vec<&str> = raw.splitn(3, ':').collect();
    match parts.as_slice() {
        [name, value] => {
            helper.completer.create_secret(name.trim(), value.trim(), None).await?;
            Ok(format!("Secret '{}' created.", name.trim()))
        }
        [name, value, desc] => {
            helper
                .completer
                .create_secret(name.trim(), value.trim(), Some(desc.trim().to_string()))
                .await?;
            Ok(format!("Secret '{}' created.", name.trim()))
        }
        _ => Err("Usage: secret-create <name>:<value>[:<description>]".into()),
    }
}
