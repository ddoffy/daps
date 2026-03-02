use crate::helper::ParamStoreHelper;

/// Handles the `migration` command.
/// Re-encrypts all locally cached parameter values with the current encryption key.
pub async fn migration(helper: &mut ParamStoreHelper) -> Result<(), Box<dyn std::error::Error>> {
    helper.completer.migrate_encryption().await?;
    println!("Migration completed");
    Ok(())
}
