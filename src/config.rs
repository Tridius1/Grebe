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
    pub select_new_app: bool,
    pub volume_scroll_size: u8,
    pub invert_volume: bool,
    pub invert_navigation: bool,
}

// Public init function, should only be called once (in main.rs)
pub fn init() {
    // Find config.toml; loop through parent directories
    let mut current_dir = env::current_exe().ok().expect("[Config] Critical Error: Could not aquire current directory.");
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
    // Read config
    let config_str = fs::read_to_string(config_file.expect("[Config] Critical Error: Failed to find config.toml"))
    .expect("[Config] Critical Error: Failed to read config.toml");
        
    // Parse config and populate GrebeConfig struct
    let parsed_config: GrebeConfig = toml::from_str(&config_str).expect("[Config] Critical Error: Failed to parse TOML config.");
    
    CONFIG.set(parsed_config).expect("[Config] Global config already initialized.");
}

// Public function; returns a static reference
pub fn get() -> &'static GrebeConfig {
    CONFIG.get().expect("Config is not initialized.")
}