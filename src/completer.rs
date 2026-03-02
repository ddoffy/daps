use crate::encryption::Encryption;
use crate::utils::replace_first_line_containing;
use rusoto_core::{Region, RusotoError};
use rusoto_ssm::{GetParameterRequest, GetParametersByPathRequest, Ssm, SsmClient};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};

pub struct ParameterCompleter {
    pub parameters: Arc<Mutex<HashMap<String, Vec<String>>>>,
    pub values: Arc<Mutex<HashMap<String, String>>>,
    pub client: SsmClient,
    pub base_path: String,
    pub refresh: bool,
    pub store_dir: String,
    pub verbose: bool,
    pub metadata: Arc<Mutex<HashMap<String, String>>>,
    pub encryption: Encryption,
    pub search_result: Arc<Mutex<Vec<String>>>,
}

impl ParameterCompleter {
    /// Creates a platform-appropriate file path for parameter storage.
    pub fn get_file_path(&self, base_path: &str, file_type: &str) -> String {
        if cfg!(target_os = "windows") {
            format!("{}\\{}_{}.txt", self.store_dir, file_type, base_path)
        } else {
            format!("{}/{}_{}.txt", self.store_dir, file_type, base_path)
        }
    }

    /// Returns a sanitized version of `base_path` (slashes replaced with underscores).
    pub fn get_sanitized_base_path(&self) -> String {
        self.base_path.replace('/', "_")
    }

    pub fn new(
        region: Region,
        base_path: String,
        refresh: bool,
        store_dir: String,
        verbose: bool,
        encryption: Encryption,
    ) -> Self {
        let client = SsmClient::new(region);
        let parameters = Arc::new(Mutex::new(HashMap::new()));
        let values = Arc::new(Mutex::new(HashMap::new()));
        let metadata = Arc::new(Mutex::new(HashMap::new()));

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
            encryption,
            search_result: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn set_parameter(
        &self,
        path: &str,
        value: String,
        param_type: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
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

        self.client.put_parameter(request).await?;
        Ok(())
    }

    pub async fn update_all(
        &self,
        path: &str,
        value: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut parameters = self.parameters.lock().unwrap();
        let mut values = self
            .values
            .lock()
            .map_err(|e| format!("Failed to lock values: {}", e))?;

        self.log(format!("Updating parameter: {}", path).as_str());
        self.log(format!("New value: {}", value).as_str());

        self.process_parameter_path(path, &mut parameters);
        values.insert(path.to_string(), value.to_string());

        let base_path = self.get_sanitized_base_path();
        let file_path = self.get_file_path(&base_path, "values");

        self.log(format!("Writing value to file: {}", file_path).as_str());

        let encrypted_value = self.encryption.encrypt_value(&value);
        let new_line = format!("{}: {}\n", path, encrypted_value);

        fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(file_path)?
            .write_all(new_line.as_bytes())?;

        self.write_parameters_to_file(base_path.as_str(), parameters.clone())?;

        self.log("Updated all parameters and values");
        Ok(())
    }

    pub async fn change_value(
        &self,
        path: &str,
        value: String,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let request = GetParameterRequest {
            name: path.to_string(),
            with_decryption: Some(true),
            ..Default::default()
        };

        self.log(format!("Fetching parameter: {}", path).as_str());

        let result = self.client.get_parameter(request).await?;

        if let Some(param) = result.parameter {
            self.set_parameter(path, value.clone(), param.type_).await?;
        }

        self.log(format!("Setting parameter: {}", path).as_str());

        let mut values = self
            .values
            .lock()
            .map_err(|e| format!("Failed to lock values: {}", e))?;
        values.insert(path.to_string(), value.clone());

        let base_path = self.get_sanitized_base_path();
        let file_path = self.get_file_path(&base_path, "values");

        let encrypted_value = self.encryption.encrypt_value(&value);
        replace_first_line_containing(
            &file_path,
            path,
            format!("{}: {}", path, encrypted_value).as_str(),
        )?;

        self.log(format!("Updated parameter: {}", path).as_str());
        Ok(value)
    }

    pub async fn get_set_values(
        &self,
        paths: &str,
    ) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
        let mut results = HashMap::new();

        let mut request = GetParametersByPathRequest {
            path: paths.to_string(),
            recursive: Some(true),
            with_decryption: Some(true),
            ..Default::default()
        };

        self.log(format!("Fetching parameters from path: {}", paths).as_str());
        let result = self.client.get_parameters_by_path(request.clone()).await?;

        if result.parameters.is_none() {
            self.log("No parameters found in the specified path");
            return Ok(results);
        }

        let mut result_parameters = result.parameters.unwrap();
        let mut next_token = result.next_token;

        while let Some(token) = next_token {
            request.next_token = Some(token);
            let next_result = self.client.get_parameters_by_path(request.clone()).await?;
            if let Some(params) = next_result.parameters {
                result_parameters.extend(params);
            }
            next_token = next_result.next_token;
        }

        for param in result_parameters {
            if let Some(name) = param.name {
                if let Some(value) = param.value {
                    results.insert(name.clone(), value.clone());
                    self.update_all(name.as_str(), value).await?;
                }
            }
        }

        self.log(format!("Fetched {} parameters", results.len()).as_str());
        if results.is_empty() {
            self.log("No parameters found in the specified path");
        } else {
            self.log("Parameters fetched successfully");
        }
        Ok(results)
    }

    pub async fn get_set_value(
        &self,
        path: &str,
    ) -> Result<String, RusotoError<rusoto_ssm::GetParameterError>> {
        self.log(format!("Fetching parameter: {}", path).as_str());

        let request = GetParameterRequest {
            name: path.to_string(),
            with_decryption: Some(true),
            ..Default::default()
        };

        self.log(format!("Fetching parameter: {}", path).as_str());
        let result = self.client.get_parameter(request).await?;

        if let Some(param) = result.parameter {
            if let Some(value) = param.value {
                {
                    let mut values = self.values.lock().unwrap();
                    values.insert(path.to_string(), value.clone());
                }

                let base_path = self.get_sanitized_base_path();
                let values_file_path = self.get_file_path(&base_path, "values");
                let parameters = self.parameters.lock().unwrap();
                let path_exists = parameters.contains_key(path);
                drop(parameters);

                let encrypted_value = self.encryption.encrypt_value(&value);

                if !path_exists {
                    match self.update_all(path, value.to_string()).await {
                        Ok(_) => {
                            self.log(format!("Added parameter: {}", path).as_str());
                        }
                        Err(e) => {
                            self.log(format!("Error adding parameter: {}", e).as_str());
                        }
                    }
                } else {
                    replace_first_line_containing(
                        &values_file_path,
                        path,
                        format!("{}: {}", path, encrypted_value).as_str(),
                    )
                    .unwrap_or_default();
                }

                self.log(format!("Updated parameter: {}", path).as_str());
                return Ok(value);
            }
        }

        self.log(format!("Parameter not found: {}", path).as_str());
        Ok("".to_string())
    }

    fn add_commands(&self, paths_map: &mut HashMap<String, Vec<String>>) {
        paths_map.insert("set".to_string(), Vec::new());
        paths_map.insert("select".to_string(), Vec::new());
        paths_map.insert("insert".to_string(), Vec::new());
        paths_map.insert("search".to_string(), Vec::new());
        paths_map.insert("refresh".to_string(), Vec::new());
        paths_map.insert("reload".to_string(), Vec::new());
        paths_map.insert("reloads".to_string(), Vec::new());
        paths_map.insert("reload-by-path".to_string(), Vec::new());
        paths_map.insert("reload-by-paths".to_string(), Vec::new());
        paths_map.insert("exit".to_string(), Vec::new());
    }

    pub async fn load_parameters(
        &self,
    ) -> Result<(), RusotoError<rusoto_ssm::GetParametersByPathError>> {
        let mut parameters = self.parameters.lock().expect("Failed to lock parameters");
        parameters.clear();

        let mut values = self.values.lock().expect("Failed to lock values");
        values.clear();

        let mut paths_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut values_d: HashMap<String, String> = HashMap::new();

        paths_map.insert(self.base_path.clone(), Vec::new());
        self.add_commands(&mut paths_map);

        let mut next_token: Option<String> = None;
        let mut is_parameters_loaded = false;
        let mut is_values_loaded = false;

        if !self.refresh {
            self.log("Checking for existing parameters and values files...");
            let base_path = self.base_path.replace('/', "_");

            if let Err(e) = self.load_parameters_from_file(base_path.as_str(), &mut paths_map) {
                self.log(format!("Error loading parameters from file: {}", e).as_str());
            } else {
                is_parameters_loaded = true;
            }

            if let Err(e) = self.load_values_from_file(base_path.as_str(), &mut values_d) {
                self.log(format!("Error loading values from file: {}", e).as_str());
            } else {
                is_values_loaded = true;
            }

            if is_parameters_loaded && is_values_loaded {
                self.log("Parameters and values loaded from file");
                self.log(
                    format!(
                        "Loaded {} parameter paths and {} values",
                        paths_map.len(),
                        values_d.len(),
                    )
                    .as_str(),
                );

                *parameters = paths_map;
                *values = values_d;
                return Ok(());
            }
        }

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
                max_results: Some(10),
                with_decryption: Some(true),
            };

            let result = self.client.get_parameters_by_path(request).await?;

            if result.parameters.is_none() {
                break;
            }

            let len = result.parameters.as_ref().map_or(0, |p| p.len());
            self.log(format!("Fetched {} parameters", len).as_str());
            total += len;
            self.log(format!("Total parameters fetched: {}", total).as_str());

            if let Some(params) = &result.parameters {
                for param in params {
                    if let Some(name) = &param.name {
                        self.process_parameter_path(name, &mut paths_map);
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

        *parameters = paths_map.clone();
        *values = values_d.clone();

        let base_path = self.base_path.replace('/', "_");

        self.log("Writing parameters and values to file...");
        self.write_parameters_to_file(base_path.as_str(), paths_map)?;
        self.write_values_to_file(base_path.as_str(), values_d)?;

        self.log(format!("Loaded {} parameter paths", parameters.len()).as_str());
        Ok(())
    }

    pub async fn migrate_encryption(&self) -> Result<(), Box<dyn std::error::Error>> {
        let base_path = self.get_sanitized_base_path();
        let file_path = self.get_file_path(&base_path, "values");

        if !std::path::Path::new(&file_path).exists() {
            return Ok(());
        }

        let file = File::open(&file_path)?;
        let reader = BufReader::new(file);
        let mut lines = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.contains(':') {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() == 2 {
                    let key = parts[0].trim();
                    let value = parts[1].trim();
                    lines.push(format!("{}: {}", key, self.encryption.encrypt_value(value)));
                }
            }
        }

        let mut file = File::create(&file_path)?;
        for line in lines {
            writeln!(file, "{}", line)?;
        }

        self.log("Migration completed");
        Ok(())
    }

    pub fn load_parameters_from_file(
        &self,
        base_path: &str,
        paths_map: &mut HashMap<String, Vec<String>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let store_dir = &self.store_dir;
        let file_path = if cfg!(target_os = "windows") {
            format!("{}\\parameters_{}.txt", store_dir, base_path)
        } else {
            format!("{}/parameters_{}.txt", store_dir, base_path)
        };

        self.log(format!("Loading parameters from file: {}", file_path).as_str());
        let file = File::open(file_path)?;
        let reader = io::BufReader::new(file);

        paths_map.insert(self.base_path.clone(), Vec::new());

        for line in reader.lines() {
            let line = line?;
            if line.contains(':') {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() == 2 {
                    let path = parts[0].trim();
                    self.process_parameter_path(path, paths_map);
                }
            }
        }

        self.log("Parameters loaded from file");
        Ok(())
    }

    pub fn load_values_from_file(
        &self,
        base_path: &str,
        values_map: &mut HashMap<String, String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let store_dir = &self.store_dir;
        let file_path = if cfg!(target_os = "windows") {
            format!("{}\\values_{}.txt", store_dir, base_path)
        } else {
            format!("{}/values_{}.txt", store_dir, base_path)
        };

        self.log(format!("Loading values from file: {}", file_path).as_str());
        let file = File::open(file_path)?;
        let reader = io::BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            if line.contains(':') {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() == 2 {
                    let key = parts[0].trim().to_owned();
                    let value = parts[1].trim().to_owned();
                    values_map.insert(key, self.encryption.decrypt_value(&value));
                }
            }
        }
        Ok(())
    }

    pub fn write_values_to_file(
        &self,
        base_path: &str,
        values: HashMap<String, String>,
    ) -> io::Result<()> {
        self.log("Writing values to file...");
        self.log(format!("Len of values: {}", values.len()).as_str());

        let store_dir = &self.store_dir;
        let file_path = if cfg!(target_os = "windows") {
            format!("{}\\values_{}.txt", store_dir, base_path)
        } else {
            format!("{}/values_{}.txt", store_dir, base_path)
        };

        self.log(format!("File path: {}", file_path).as_str());

        let mut file = File::create(file_path)?;
        for (key, value) in values.iter() {
            let encrypted_value = self.encryption.encrypt_value(value);
            writeln!(file, "{}: {}", key, encrypted_value)?;
        }

        self.log("Values written to file");
        Ok(())
    }

    pub fn write_parameters_to_file(
        &self,
        base_path: &str,
        parameters: HashMap<String, Vec<String>>,
    ) -> io::Result<()> {
        self.log("Writing parameters to file...");
        self.log(format!("Len of parameters: {}", parameters.len()).as_str());

        let store_dir = &self.store_dir;
        let file_path = if cfg!(target_os = "windows") {
            format!("{}\\parameters_{}.txt", store_dir, base_path)
        } else {
            format!("{}/parameters_{}.txt", store_dir, base_path)
        };

        let mut file = File::create(file_path)?;
        for (path, children) in parameters.iter() {
            writeln!(file, "{}: {:?}", path, children)?;
        }

        self.log("Parameters written to file");
        Ok(())
    }

    pub fn process_parameter_path(
        &self,
        full_path: &str,
        paths_map: &mut HashMap<String, Vec<String>>,
    ) {
        paths_map.entry("/".to_string()).or_default();

        let path_parts: Vec<&str> = full_path
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        let mut current_path = "/".to_string();

        for part in path_parts {
            paths_map
                .entry(current_path.clone())
                .or_default()
                .push(part.to_string());

            if current_path.ends_with('/') {
                current_path.push_str(part);
            } else {
                current_path.push('/');
                current_path.push_str(part);
            }

            paths_map.entry(current_path.clone()).or_default();
        }
    }

    pub fn get_completions(&self, path: &str) -> Vec<String> {
        let Ok(parameters) = self.parameters.lock() else {
            return Vec::new();
        };
        let Ok(metadata) = self.metadata.lock() else {
            return Vec::new();
        };

        if path.to_lowercase().starts_with("set") {
            let Ok(values) = self.values.lock() else {
                return Vec::new();
            };
            let selected = metadata
                .get("selected")
                .unwrap_or(&"".to_string())
                .to_string();
            let val = values.get(&selected).unwrap_or(&"".to_string()).to_string();
            return vec![format!("set {}", val)];
        }

        if path.to_lowercase().starts_with("insert") {
            let Ok(values) = self.values.lock() else {
                return Vec::new();
            };
            let selected = metadata
                .get("selected")
                .unwrap_or(&"".to_string())
                .to_string();
            let val = values.get(&selected).unwrap_or(&"".to_string()).to_string();
            return vec![format!("insert {}:{}:{}", selected, val, "String")];
        }

        let lookup_path = if path.is_empty() || !path.contains('/') {
            "/".to_string()
        } else {
            let last_slash = path.rfind('/').unwrap_or(0);
            if last_slash == 0 {
                "/".to_string()
            } else {
                path[0..last_slash].to_string()
            }
        };

        let prefix = if path.contains('/') {
            path.split('/').last().unwrap_or("").to_string()
        } else {
            path.to_string()
        };

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

    pub fn log(&self, message: &str) {
        if self.verbose {
            println!("{}", message);
        }
    }
}
