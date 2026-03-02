use crate::helper::ParamStoreHelper;

/// Handles the `refresh` command.
/// Reloads all parameters from AWS SSM, bypassing the local cache.
pub async fn refresh(helper: &mut ParamStoreHelper) -> Result<(), Box<dyn std::error::Error>> {
    helper
        .completer
        .load_parameters()
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    println!("Parameters refreshed");
    Ok(())
}
