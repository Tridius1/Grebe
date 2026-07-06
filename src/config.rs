use std::fs;
use serde::Deserialize;
use std::sync::OnceLock;

// Global config struct so threads and modules can all use this (modules use: "use crate::config;")
static CONFIG: OnceLock<GrebeConfig> = OnceLock::new();

#[derive(Debug, Deserialize)]
pub struct GrebeConfig {
    pub verbose: bool,
}

// Public init function, should only be called once (in main.rs)
pub fn init() {
    // Read config
    let config_str = fs::read_to_string("config.toml")
        .expect("Critical: Failed to read config.toml");
        
    // Parse config and populate GrebeConfig struct
    let parsed_config: GrebeConfig = toml::from_str(&config_str)
        .expect("Critical: Failed to parse TOML config");
    
    CONFIG.set(parsed_config).expect("Global config already initialized!");
}

// Public function; returns a static reference
pub fn get() -> &'static GrebeConfig {
    CONFIG.get().expect("Config is not initialized")
}