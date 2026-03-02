pub mod insert;
pub mod migration;
pub mod parse_db;
pub mod refresh;
pub mod reload;
pub mod reload_by_paths;
pub mod search;
pub mod select;
pub mod set;

use crate::cpboard::Cpboard;

pub async fn handle_command_result<'a>(
    result: Result<String, Box<dyn std::error::Error>>,
    cpboard: &mut Cpboard<'a>,
) {
    match result {
        Ok(value) => {
            use colored::Colorize;
            println!("Result value: {}", value.red());
            match cpboard.set_clipboard_content(&value) {
                Ok(_) => println!("Copied to clipboard: {}", value.red()),
                Err(err) => println!("Error copying to clipboard: {}", err),
            }
        }
        Err(err) => {
            println!("Error executing command: {}", err);
        }
    }
}
