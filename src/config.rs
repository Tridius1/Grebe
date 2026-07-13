use std::fs;
use serde::Deserialize;
use std::sync::OnceLock;
use std::env;

// Global config struct so threads and modules can all use this (modules use: "use crate::config;")
static CONFIG: OnceLock<GrebeConfig> = OnceLock::new();

#[derive(Debug, Deserialize)]
pub struct GrebeConfig {
    pub verbose: bool,
    pub port: String,
    pub blacklist: Vec<String>,
    pub aliases: Vec<(String, String)>,
    pub select_new_app: bool,
    pub volume_scroll_size: u8,
    pub invert_volume: bool,
    pub invert_navigation: bool,
}

// Public init function, should only be called once (in main.rs)
pub fn init() {
    // Read config if exists
    let config_str = find_config();
    // Parse config and populate GrebeConfig struct
    let parsed_config: GrebeConfig =  match config_str {
        Some(cfg_str) => {
            let parsed = toml::from_str(&cfg_str);
            match parsed {
                Ok(grebe_config) => grebe_config,
                Err(e) => {
                    eprintln!("[Config] Failed to deserialize config.toml:\n{}", e);
                    toml::from_str(create_default_config(true)).expect("[Config] Failed to deserialize default config")
                }
            }
        },
        None => {
            eprintln!("[Config] Could not find config.toml");
            toml::from_str(create_default_config(false)).expect("[Config] Failed to deserialize default config")
        }
    };
    
    
    CONFIG.set(parsed_config).expect("[Config] Global config already initialized");
}

// Public function; returns a static reference
pub fn get() -> &'static GrebeConfig {
    CONFIG.get().expect("Config is not initialized")
}

// Find config.toml 
fn find_config() -> Option<String> {
    // Find config.toml; loop through parent directories
    let mut current_dir = env::current_exe().ok()?;
    current_dir.pop();
    let config_file = loop {
        let candidate = current_dir.join("config.toml");
        if candidate.is_file() {
            break Some(candidate)
        }

        // Move up one directory. If there are no more parents, stop.
        if !current_dir.pop() {
            break None
        }
    };
    let config_str = fs::read_to_string(config_file?);
    match config_str {
        Ok(cfg_str) => Some(cfg_str),
        Err(_) => None,
    }
}

fn create_default_config(config_exists: bool) -> &'static str {
    // Includes default config in binary
    let default_config = include_str!("default_config.toml");
    let file_name = if config_exists {"default_config.toml"} else {"config.toml"};
    match fs::write(file_name, default_config) {
        Ok(()) => { eprintln!("[Config] Created default config file at {}", file_name); },
        Err(e) => { eprintln!("[Config] Failed to create default config file: {}", e); }
    }
    default_config
}