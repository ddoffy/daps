pub mod insert;
pub mod migration;
pub mod parse_db;
pub mod refresh;
pub mod reload;
pub mod reload_by_paths;
pub mod search;
pub mod select;
pub mod set;

pub async fn handle_command_result(
    result: Result<String, Box<dyn std::error::Error>>,
) {
    match result {
        Ok(value) => {
            use colored::Colorize;
            println!("{}", value.red());
        }
        Err(err) => {
            println!("Error executing command: {}", err);
        }
    }
}
