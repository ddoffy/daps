use crate::helper::ParamStoreHelper;
use colored::Colorize;

/// Handles `sel <index>` – selects a parameter from the last search results by numeric index.
/// Returns the selected parameter path, or an error if the index is invalid.
pub fn select_by_index(
    helper: &mut ParamStoreHelper,
    arg: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    if arg.is_empty() {
        return Err("No parameter selected".into());
    }

    let index = arg
        .parse::<usize>()
        .map_err(|_| "Argument must be a numeric index from search results")?;

    let search_result = helper
        .completer
        .search_result
        .lock()
        .map_err(|_| "Failed to access search results")?
        .clone();

    if index >= search_result.len() {
        return Err("Invalid index selected".into());
    }

    let selected_param = search_result[index].clone();

    if let Ok(mut metadata) = helper.completer.metadata.lock() {
        metadata.insert("selected".to_string(), selected_param.clone());
    }

    println!("Selected parameter: {}", selected_param.green());
    Ok(selected_param)
}
