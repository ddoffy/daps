use crate::encryption::{decrypt_value, encrypt_value};
use clipboard::{ClipboardContext, ClipboardProvider};
use colored::Colorize;
use rusoto_core::{Region, RusotoError};
use rusoto_ssm::{GetParameterRequest, GetParametersByPathRequest, Ssm, SsmClient};
use rustyline::{
    CompletionType, Config, Context, EditMode, Editor, Helper,
    completion::{Completer, Pair},
    error::ReadlineError,
    highlight::{Highlighter, MatchingBracketHighlighter},
    hint::{Hint, Hinter},
    validate::Validator,
};
use std::borrow::Cow::{self, Borrowed, Owned};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex};
use structopt::StructOpt;

pub mod encryption;

// Encrypt and decrypt keys constant
const ENCRYPTION_KEY: &str = "my_secret_key"; // Replace with your actual key

#[derive(Debug, StructOpt)]
#[structopt(
    name = "daps",
    about = "D. AWS Parameter Store CLI with tab completion",
    author = "D. Doffy <cuongnsm@gmail.com>"
)]
struct Opt {
    /// AWS Region
    #[structopt(long, default_value = "us-east-1")]
    region: String,

    /// Starting path for parameter store (e.g., /prod/)
    #[structopt(short, long, default_value = "/")]
    path: String,

    /// Refresh parameter cache
    #[structopt(short, long)]
    refresh: bool,

    /// Store directory for parameters and values
    #[structopt(long, default_value = "parameters")]
    store_dir: String,

    /// Verbose output
    #[structopt(long)]
    verbose: bool,
}

// Helper structure for rustyline that provides parameter completion
struct ParameterCompleter {
    parameters: Arc<Mutex<HashMap<String, Vec<String>>>>,
    values: Arc<Mutex<HashMap<String, String>>>,
    client: SsmClient,
    base_path: String,
    refresh: bool,
    store_dir: String,
    verbose: bool,
    metadata: Arc<Mutex<HashMap<String, String>>>,
}

impl ParameterCompleter {
    fn new(
        region: Region,
        base_path: String,
        refresh: bool,
        store_dir: String,
        verbose: bool,
    ) -> Self {
        let client = SsmClient::new(region);
        let parameters = Arc::new(Mutex::new(HashMap::new()));
        let values = Arc::new(Mutex::new(HashMap::new()));
        let metadata = Arc::new(Mutex::new(HashMap::new()));
        // Create the directory if it doesn't exist
        std::fs::create_dir_all(&store_dir).unwrap_or_else(|_| {
            println!("Failed to create directory: {}", store_dir);
        });

        Self {
            parameters,
            client,
            base_path,
            values,
            refresh,
            store_dir,
            verbose,
            metadata,
        }
    }

    async fn set_parameter(
        &self,
        path: &str,
        value: String,
        param_type: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Set the parameter with the new value
        let request = rusoto_ssm::PutParameterRequest {
            name: path.to_string(),
            value,
            overwrite: Some(true),
            type_: param_type,
            tier: None,
            data_type: None,
            allowed_pattern: None,
            description: None,
            key_id: None,
            policies: None,
            tags: None,
        };

        // Send the request to AWS
        self.client.put_parameter(request).await?;

        Ok(())
    }

    async fn update_all(
        &self,
        path: &str,
        value: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Update all parameters with the new value
        let mut parameters = self.parameters.lock().unwrap();
        let mut values = self.values.lock().unwrap();

        self.log(format!("Updating parameter: {}", path).as_str());
        self.log(format!("New value: {}", value).as_str());

        // add the parameter to the parameters map
        self.process_parameter_path(path, &mut parameters);

        // add the value to the values map
        values.insert(path.to_string(), value.to_string());

        // Write the updated value to the file
        let base_path = self.base_path.clone().replace('/', "_");
        let file_path = format!("{}/values_{}.txt", self.store_dir, base_path);

        self.log(format!("Writing value to file: {}", file_path).as_str());

        // encrypt the value before writing to the file
        let encrypted_value = encrypt_value(&value);

        // new line to insert, append to the file
        let new_line = format!("{}: {}\n", path, encrypted_value);

        fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(file_path)?
            .write_all(new_line.as_bytes())?;

        // Write the parameters to the file
        self.write_parameters_to_file(base_path.as_str(), parameters.clone())?;

        self.log("Updated all parameters and values");

        Ok(())
    }

    async fn change_value(
        &self,
        path: &str,
        value: String,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // Check if the parameter exists to get its type
        let request = GetParameterRequest {
            name: path.to_string(),
            with_decryption: Some(true),
            ..Default::default()
        };

        // Fetch the parameter from AWS
        self.log(format!("Fetching parameter: {}", path).as_str());

        let result = self.client.get_parameter(request).await?;

        if let Some(param) = result.parameter {
            self.set_parameter(path, value.clone(), param.type_).await?;
        }

        self.log(format!("Setting parameter: {}", path).as_str());

        // Update the values map with the new value
        let mut values = self.values.lock().unwrap();
        values.insert(path.to_string(), value.clone());

        // Write the updated value to the file
        let base_path = self.base_path.clone().replace('/', "_");
        let file_path = format!("{}/values_{}.txt", self.store_dir, base_path);
        // find the line index with the key in the file

        // encrypt the value before writing to the file
        let encrypted_value = encrypt_value(&value);

        replace_first_line_containing(
            &file_path,
            path,
            format!("{}: {}", path, encrypted_value).as_str(),
        )?;

        self.log(format!("Updated parameter: {}", path).as_str());

        Ok(value)
    }

    async fn get_set_value(
        &self,
        path: &str,
    ) -> Result<String, RusotoError<rusoto_ssm::GetParameterError>> {
        self.log(format!("Fetching parameter: {}", path).as_str());
        // get value from AWS parameter store
        let request = GetParameterRequest {
            name: path.to_string(),
            with_decryption: Some(true),
            ..Default::default()
        };

        // Fetch the parameter from AWS
        self.log(format!("Fetching parameter: {}", path).as_str());
        let result = self.client.get_parameter(request).await?;

        if let Some(param) = result.parameter {
            if let Some(value) = param.value {
                // Store the value in the values map
                self.values
                    .lock()
                    .unwrap()
                    .insert(path.to_string(), value.clone());

                // Write the updated value to the file
                let base_path = self.base_path.clone().replace('/', "_");
                let file_path = format!("{}/values_{}.txt", self.store_dir, base_path);
                // find the line index with the key in the file

                // encrypt the value before writing to the file
                let encrypted_value = encrypt_value(&value);

                replace_first_line_containing(
                    &file_path,
                    path,
                    format!("{}: {}", path, encrypted_value).as_str(),
                )?;

                self.log(format!("Updated parameter: {}", path).as_str());

                return Ok(value);
            }
        }

        self.log(format!("Parameter not found: {}", path).as_str());

        Ok("".to_string())
    }

    fn add_commands(&self, paths_map: &mut HashMap<String, Vec<String>>) {
        // Add commands to the parameters map
        paths_map.insert("set".to_string(), Vec::new());
        paths_map.insert("insert".to_string(), Vec::new());
        paths_map.insert("search".to_string(), Vec::new());
        paths_map.insert("refresh".to_string(), Vec::new());
        paths_map.insert("reload".to_string(), Vec::new());
        paths_map.insert("exit".to_string(), Vec::new());
    }

    async fn load_parameters(
        &self,
    ) -> Result<(), RusotoError<rusoto_ssm::GetParametersByPathError>> {
        let mut parameters = self.parameters.lock().unwrap();
        parameters.clear();

        let mut values = self.values.lock().unwrap();
        values.clear();

        // Create a HashMap to store paths and their children
        let mut paths_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut values_d: HashMap<String, String> = HashMap::new();

        // Initialize with the base path
        paths_map.insert(self.base_path.clone(), Vec::new());
        self.add_commands(&mut paths_map);

        // Fetch all parameters recursively
        let mut next_token: Option<String> = None;

        let mut is_parameters_loaded = false;
        let mut is_values_loaded = false;

        // ignore if the refresh flag is set
        if !self.refresh {
            // Check if the parameters and values file exists
            self.log("Checking for existing parameters and values files...");
            let base_path = self.base_path.clone().replace('/', "_");

            // if parameters file exists, load them
            if let Err(e) = self.load_parameters_from_file(base_path.as_str(), &mut paths_map) {
                self.log(format!("Error loading parameters from file: {}", e).as_str());
            } else {
                is_parameters_loaded = true;
            }

            // if values file exists, load them
            if let Err(e) = self.load_values_from_file(base_path.as_str(), &mut values_d) {
                self.log(format!("Error loading values from file: {}", e).as_str());
            } else {
                is_values_loaded = true;
            }

            if is_parameters_loaded && is_values_loaded {
                // if both exist, return
                self.log("Parameters and values loaded from file");

                self.log(
                    format!(
                        "Loaded {} parameter paths and {} values",
                        paths_map.len(),
                        values_d.len(),
                    )
                    .as_str(),
                );

                // Update the shared parameters and values map
                *parameters = paths_map;
                *values = values_d;

                return Ok(());
            }
        }

        // if both does not exist, fetch from AWS
        self.log(
            format!(
                "Loading parameters from AWS Parameter Store from path {} ...",
                self.base_path
            )
            .as_str(),
        );

        let mut total = 0;

        loop {
            let request = GetParametersByPathRequest {
                path: self.base_path.clone(),
                recursive: Some(true),
                parameter_filters: None,
                next_token: next_token.clone(),
                max_results: Some(10), // Adjust based on your needs
                with_decryption: Some(true),
            };

            let result = self.client.get_parameters_by_path(request).await?;

            // Check if we have reached the end of the results
            if result.parameters.is_none() {
                break;
            }

            let len = result.parameters.as_ref().unwrap().len();
            self.log(format!("Fetched {} parameters", len).as_str());

            total += len;
            self.log(format!("Total parameters fetched: {}", total).as_str());

            if let Some(params) = &result.parameters {
                for param in params {
                    if let Some(name) = &param.name {
                        // Process each parameter path and add to our map
                        self.process_parameter_path(name, &mut paths_map);
                        // Store the parameter value in the values map
                        if let Some(value) = &param.value {
                            values_d.insert(name.clone(), value.clone());
                        }
                    }
                }
            }

            next_token = result.next_token;

            if next_token.is_none() {
                break;
            }
        }

        // Update the shared parameters map
        *parameters = paths_map.clone();
        *values = values_d.clone();

        let base_path = self.base_path.clone();
        // Write the values to a file to persist them
        let base_path = base_path.replace('/', "_");

        // Write the parameters and values to a file to persist them
        // avoid reloading them every time
        // This is a placeholder for file writing logic
        // You can use serde_json or any other method to serialize the data
        // serialize the parameters and values to a file
        self.log("Writing parameters and values to file...");
        // write both parameters and values to a file at the same time

        self.write_parameters_to_file(base_path.as_str(), paths_map)?;
        // write the values to a file
        self.write_values_to_file(base_path.as_str(), values_d)?;

        self.log(format!("Loaded {} parameter paths", parameters.len()).as_str());
        Ok(())
    }

    fn load_parameters_from_file(
        &self,
        base_path: &str,
        paths_map: &mut HashMap<String, Vec<String>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Load parameters from a file
        let store_dir = self.store_dir.clone();
        let file_path = format!("{}/parameters_{}.txt", store_dir, base_path);
        self.log(format!("Loading parameters from file: {}", file_path).as_str());
        let file = File::open(file_path)?;
        let reader = io::BufReader::new(file);

        // Initialize with the base path
        paths_map.insert(self.base_path.clone(), Vec::new());

        for line in reader.lines() {
            let line = line?;
            if line.contains(':') {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() == 2 {
                    let path = parts[0].trim().to_string();
                    self.process_parameter_path(&path, paths_map);
                }
            }
        }

        self.log("Parameters loaded from file");

        Ok(())
    }

    fn load_values_from_file(
        &self,
        base_path: &str,
        values_map: &mut HashMap<String, String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Load values from a file
        let store_dir = self.store_dir.clone();
        let file_path = format!("{}/values_{}.txt", store_dir, base_path);
        self.log(format!("Loading values from file: {}", file_path).as_str());
        let file = File::open(file_path)?;
        let reader = io::BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            if line.contains(':') {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() == 2 {
                    let key = parts[0].trim().to_string();
                    let value = parts[1].trim().to_string();
                    // decrypt the value before storing it
                    let decrypted_value = decrypt_value(&value);
                    values_map.insert(key, decrypted_value);
                }
            }
        }
        Ok(())
    }

    fn write_values_to_file(
        &self,
        base_path: &str,
        values: HashMap<String, String>,
    ) -> io::Result<()> {
        self.log("Writing values to file...");
        self.log(format!("Len of values: {}", values.len()).as_str());
        let store_dir = self.store_dir.clone();
        let file_path = format!("{}/values_{}.txt", store_dir, base_path);

        self.log(format!("File path: {}", file_path).as_str());
        // Open a file to write the parameters and values
        let mut file = File::create(file_path)?;

        // Write the values
        for (key, value) in values.iter() {
            // encrypt the value before writing to the file
            let encrypted_value = encrypt_value(value);
            writeln!(file, "{}: {}", key, encrypted_value)?;
        }

        self.log("Values written to file");

        Ok(())
    }

    fn write_parameters_to_file(
        &self,
        base_path: &str,
        parameters: HashMap<String, Vec<String>>,
    ) -> io::Result<()> {
        self.log("Writing parameters to file...");
        self.log(format!("Len of parameters: {}", parameters.len()).as_str());
        let store_dir = self.store_dir.clone();
        let file_path = format!("{}/parameters_{}.txt", store_dir, base_path);
        // Open a file to write the parameters and values
        let mut file = File::create(file_path)?;
        // Write the parameters
        for (path, children) in parameters.iter() {
            writeln!(file, "{}: {:?}", path, children)?;
        }

        self.log("Parameters written to file");

        Ok(())
    }

    fn process_parameter_path(
        &self,
        full_path: &str,
        paths_map: &mut HashMap<String, Vec<String>>,
    ) {
        // Ensure the root path exists in the map
        paths_map.entry("/".to_string()).or_default();

        // Split the path into components
        let path_parts: Vec<&str> = full_path
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        let mut current_path = "/".to_string();

        // Process each part of the path
        for part in path_parts {
            // Add this part to its parent's children
            paths_map
                .entry(current_path.clone())
                .or_default()
                .push(part.to_string());

            // Update current path
            if current_path.ends_with('/') {
                current_path.push_str(part);
            } else {
                current_path.push('/');
                current_path.push_str(part);
            }

            // Ensure the current path exists in the map
            paths_map.entry(current_path.clone()).or_default();
        }
    }

    fn get_completions(&self, path: &str) -> Vec<String> {
        let parameters = self.parameters.lock().unwrap();
        let metadata = self.metadata.lock().unwrap();

        // check if the path contains commands
        if path.to_lowercase().starts_with("set") {
            // get value from the values map
            let values = self.values.lock().unwrap();
            let selected = metadata
                .get("selected")
                .unwrap_or(&"".to_string())
                .to_string();

            let val = values.get(&selected).unwrap_or(&"".to_string()).to_string();

            let set_inst = format!("set {}", val);

            return vec![set_inst];
        }

        // check if the use wants to insert a value
        if path.to_lowercase().starts_with("insert") {
            // get value from the values map
            let values = self.values.lock().unwrap();
            let selected = metadata
                .get("selected")
                .unwrap_or(&"".to_string())
                .to_string();

            let val = values.get(&selected).unwrap_or(&"".to_string()).to_string();

            let insert_inst = format!("insert {}:{}:{}", selected, val, "String");

            return vec![insert_inst];
        }

        // Determine the path to look up
        let lookup_path = if path.is_empty() || !path.contains('/') {
            "/".to_string()
        } else {
            // Extract the parent path
            let last_slash = path.rfind('/').unwrap();
            if last_slash == 0 {
                "/".to_string()
            } else {
                path[0..last_slash].to_string()
            }
        };

        // Get prefix for filtering completions
        let prefix = if path.contains('/') {
            let parts: Vec<&str> = path.split('/').collect();
            parts.last().unwrap_or(&"").to_string()
        } else {
            path.to_string()
        };

        // Look up completions in our map
        parameters
            .get(&lookup_path)
            .map(|children| {
                children
                    .iter()
                    .filter(|child| child.to_lowercase().starts_with(&prefix.to_lowercase()))
                    .map(|child| {
                        if lookup_path == "/" {
                            format!("/{}", child)
                        } else {
                            format!("{}/{}", lookup_path, child)
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn log(&self, message: &str) {
        if self.verbose {
            println!("{}", message);
        }
    }
}

// Helper implementation for rustyline
struct ParamStoreHelper {
    completer: ParameterCompleter,
    highlighter: MatchingBracketHighlighter,
    commands: Vec<String>,
}

impl Completer for ParamStoreHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        // For simplicity, we'll assume the entire line is a parameter path
        let path = line[..pos].trim();
        let start = 0; // Start completing from the beginning of the line

        let completions = self.completer.get_completions(path);

        let mut candidates: Vec<Pair> = completions
            .into_iter()
            .map(|s| Pair {
                display: s.clone(),
                replacement: s,
            })
            .collect();

        let cmd_candidates: Vec<Pair> = self
            .commands
            .iter()
            .filter(|cmd| cmd.to_lowercase().starts_with(&path.to_lowercase()))
            .map(|s| Pair {
                display: s.clone(),
                replacement: s.clone(),
            })
            .collect();

        candidates.extend(cmd_candidates);

        Ok((start, candidates))
    }
}

impl Highlighter for ParamStoreHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        use colored::*; // Bring the trait into scope

        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        let command = parts.first().unwrap_or(&"");
        let args = parts.get(1).unwrap_or(&"");

        if self.commands.contains(&command.to_lowercase()) {
            // Highlight command in blue and arguments in default color
            Owned(format!("{} {}", command.blue(), args))
        } else {
            // Default: no highlighting
            Borrowed(line)
        }
    }

    fn highlight_char(&self, _line: &str, _pos: usize) -> bool {
        // Enable highlighting per character if needed, e.g., for matching brackets
        self.highlighter.highlight_char(_line, _pos)
    }
}

// Empty string implementation for Hint
struct EmptyHint;

impl Hint for EmptyHint {
    fn display(&self) -> &str {
        ""
    }

    fn completion(&self) -> Option<&str> {
        Some("")
    }
}

impl Hinter for ParamStoreHelper {
    type Hint = EmptyHint;

    fn hint(&self, _line: &str, _pos: usize, _ctx: &Context<'_>) -> Option<Self::Hint> {
        None
    }
}

impl Validator for ParamStoreHelper {}

impl Helper for ParamStoreHelper {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::from_args();

    let region = parse_region(&opt.region).map_err(|e| format!("Invalid region: {}", e))?;

    let base_path = opt.path.clone();

    // Check if the base path is valid
    if !base_path.starts_with('/') {
        return Err("Base path must start with '/'".into());
    }

    // Create the parameter completer
    let completer =
        ParameterCompleter::new(region, base_path, opt.refresh, opt.store_dir, opt.verbose);

    // Load parameters initially
    completer.load_parameters().await?;

    // Create the line editor
    let helper = ParamStoreHelper {
        completer,
        highlighter: MatchingBracketHighlighter::new(),
        commands: vec![
            "exit".to_string(),
            "refresh".to_string(),
            "reload".to_string(),
            "set".to_string(),
            "insert".to_string(),
            "search".to_string(),
        ],
    };

    let config = Config::builder()
        .completion_type(CompletionType::Circular)
        .auto_add_history(true)
        .bell_style(rustyline::config::BellStyle::None)
        .build();

    let mut rl = Editor::with_config(config)?;
    rl.set_helper(Some(helper));

    println!("AWS Parameter Store CLI");
    println!(
        "Type a parameter path and use {} for completion",
        "Tab".red()
    );
    println!("Type '{}' to quit", "exit".yellow());

    // Create clipboard context
    let mut ctx = ClipboardProvider::new().unwrap();
    let mut cpboard = Cpboard::new(&mut ctx);

    let mut selected = String::new();
    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                match line.as_str() {
                    "exit" => break,
                    "refresh" => {
                        if let Some(helper) = rl.helper_mut() {
                            match helper.completer.load_parameters().await {
                                Ok(_) => println!("Parameters refreshed"),
                                Err(err) => println!("Error refreshing parameters: {}", err),
                            }
                        }
                        continue;
                    }
                    "reload" => {
                        if let Some(helper) = rl.helper_mut() {
                            handle_command_result(reload(helper, &selected).await, &mut cpboard)
                                .await;
                        }
                        continue;
                    }
                    cmd if cmd.starts_with("set") => {
                        if let Some(helper) = rl.helper_mut() {
                            handle_command_result(
                                set_value(helper, &line, &selected).await,
                                &mut cpboard,
                            )
                            .await;
                        }
                        continue;
                    }
                    cmd if cmd.starts_with("insert") => {
                        if let Some(helper) = rl.helper_mut() {
                            handle_command_result(insert_value(helper, &line).await, &mut cpboard)
                                .await;
                        }
                        continue;
                    }
                    cmd if cmd.starts_with("search") => {
                        if let Some(helper) = rl.helper_mut() {
                            let search_term = line.replace("search ", "");
                            let parameters = helper.completer.values.lock().unwrap();

                            let keys: Vec<_> = parameters
                                .keys()
                                .filter(|k| k.to_lowercase().contains(&search_term))
                                .collect();

                            if keys.is_empty() {
                                println!("No matching parameters found");
                            } else {
                                println!("Matching parameters:");
                                for key in keys {
                                    let value = parameters.get(key).unwrap();
                                    println!("{} -> {}", key, value.red());
                                }
                            }
                        }
                        continue;
                    }
                    _ => {}
                }

                rl.add_history_entry(line.as_str());
                selected = line.clone();

                rl.helper()
                    .unwrap()
                    .completer
                    .metadata
                    .lock()
                    .unwrap()
                    .insert("selected".to_string(), selected.clone());

                // print the value of the selected parameter
                rl.helper()
                    .unwrap()
                    .completer
                    .values
                    .lock()
                    .unwrap()
                    .get(&line)
                    .map(|v| {
                        // print with colored output to highlight the parameter
                        println!("You selected: {}", selected.green());
                        println!("Value: {}", v.red());
                        // Copy to clipboard
                        if let Err(err) = cpboard.set_clipboard_content(v) {
                            println!("Error copying to clipboard: {}", err);
                        } else {
                            println!("Copied to clipboard: {}", v);
                        }
                    });
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }

    Ok(())
}

async fn handle_command_result<'a>(
    result: Result<String, Box<dyn std::error::Error>>,
    cpboard: &mut Cpboard<'a>,
) {
    match result {
        Ok(value) => {
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

async fn insert_value(
    helper: &mut ParamStoreHelper,
    line: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    println!("Inserting parameter: {}", line);
    let path_and_value = line.replace("insert ", "").trim_start().to_string();

    // path and value: /path/to/parameter:value
    // find the first index ':' in the string
    let index = path_and_value.find(':').ok_or("Invalid format")?;
    // find the last index ':' in the string to detect parameter type
    let last_index = path_and_value.rfind(':').ok_or("Invalid format")?;
    // if the last index is not equal to the first index, then it is a parameter type
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

    helper.completer.update_all(path, value.to_string()).await?;

    // fetch the selected parameter from AWS
    println!("Inserted value: {}", value);

    Ok(value.to_string())
}

async fn set_value(
    helper: &mut ParamStoreHelper,
    line: &str,
    path: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    println!("Setting parameter: {}", path);
    let value = line.replace("set ", "");

    // fetch the selected parameter from AWS
    let value = helper.completer.change_value(path, value).await?;
    println!("Set value: {}", value);

    Ok(value)
}

async fn reload(
    helper: &mut ParamStoreHelper,
    path: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    println!("Reloading parameter: {}", path);
    // fetch the selected parameter from AWS
    let value = helper.completer.get_set_value(path).await?;
    println!("Reloaded value: {}", value);

    Ok(value)
}

fn parse_region(region: &str) -> Result<Region, String> {
    match region
        .parse::<Region>()
        .map_err(|_| format!("Invalid region: {}", region))
    {
        Ok(region) => Ok(region),
        Err(err) => Err(format!("Error parsing region: {}", err)),
    }
}

// Debug implementation for ParamStoreHelper
impl std::fmt::Debug for ParamStoreHelper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ParamStoreHelper")
    }
}

/// Replaces the first line that matches a criteria and exits immediately
fn replace_first_matching_line(
    filepath: &str,
    line_matcher: impl Fn(&str) -> bool,
    replacement_line: &str,
) -> io::Result<bool> {
    // Open the file for reading and writing
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(filepath)?;

    let mut reader = BufReader::new(&file);

    // Track position and if we found a match
    let mut current_pos: u64 = 0;
    let mut found_match = false;
    let mut line = String::new();

    // Read the file line by line
    while reader.read_line(&mut line)? > 0 {
        if !found_match && line_matcher(&line) {
            // Line matches, prepare to replace it
            found_match = true;

            // Get a mutable reference to the underlying file
            let mut file = reader.into_inner();

            // Seek to the position of the line we want to replace
            file.seek(SeekFrom::Start(current_pos))?;

            // Ensure replacement line has a newline
            let mut replacement = replacement_line.to_string();
            if !replacement.ends_with('\n') {
                replacement.push('\n');
            }

            // Write the replacement
            file.write_all(replacement.as_bytes())?;

            // If the replacement is shorter than the original, we need to handle that
            if replacement.len() < line.len() {
                // Create padding with spaces
                let padding = " ".repeat(line.len() - replacement.len());
                file.write_all(padding.as_bytes())?;
            }

            // We're done - no need to process more lines
            break;
        }

        // Update position for the next line
        current_pos += line.len() as u64;
        line.clear();
    }

    Ok(found_match)
}

/// Convenience function to replace the first line containing a substring
fn replace_first_line_containing(
    filepath: &str,
    search_text: &str,
    replacement_line: &str,
) -> io::Result<bool> {
    replace_first_matching_line(
        filepath,
        |line| line.contains(search_text),
        replacement_line,
    )
}

struct Cpboard<'a> {
    ctx: &'a mut ClipboardContext,
}

impl<'a> Cpboard<'a> {
    fn new(ctx: &'a mut ClipboardContext) -> Cpboard<'a> {
        Cpboard { ctx }
    }

    fn set_clipboard_content(&mut self, content: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.ctx.set_contents(content.to_owned())?;
        Ok(())
    }

    #[allow(dead_code)]
    fn get_clipboard_content(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        self.ctx.get_contents()
    }
}
