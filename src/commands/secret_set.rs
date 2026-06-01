use crate::helper::ParamStoreHelper;

pub async fn secret_set(
    helper: &mut ParamStoreHelper,
    name: &str,
    value: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    helper.completer.set_secret(name, value).await?;
    Ok(format!("Secret '{}' updated.", name))
}
