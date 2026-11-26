use crate::helper::ParamStoreHelper;
use colored::Colorize;

pub async fn secret_list(
    helper: &ParamStoreHelper,
    filter: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let secrets = helper.completer.list_secrets_from_aws(filter).await?;
    if secrets.is_empty() {
        return Ok("No secrets found.".to_string());
    }
    let mut out = String::new();
    for (name, desc) in &secrets {
        if desc.is_empty() {
            out.push_str(&format!("{}\n", name.green()));
        } else {
            out.push_str(&format!("{}  {}\n", name.green(), desc.dimmed()));
        }
    }
    Ok(out.trim_end().to_string())
}
