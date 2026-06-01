use crate::helper::ParamStoreHelper;

pub async fn secret_get(
    helper: &mut ParamStoreHelper,
    name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    helper.completer.get_secret(name).await
}
