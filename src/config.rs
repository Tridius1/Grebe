use std::fs;
use serde::Deserialize;
use std::sync::OnceLock;
use std::env;
use log::{info, error};

// Global config struct so threads and modules can all use this (modules use: "use crate::config;")
static CONFIG: OnceLock<GrebeConfig> = OnceLock::new();

pub static CONFIG_SIZE: usize = 6; // size of packet containing config for microcontroller

#[derive(Debug, Deserialize, Clone)]
pub struct NotificationConfig {
    pub on_first_connect: bool,
    pub on_reconnect: bool,
    pub on_disconnect: bool,
    pub silent: bool,
    pub expiration: i64
}

#[derive(Debug, Deserialize, Clone)]
pub struct DisplayConfig {
    pub invert: bool,
    pub text_color: u64,
    pub background_color: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GrebeConfig {
    pub verbose: bool,
    pub port: String,
    pub blacklist: Vec<String>,
    pub aliases: Vec<(String, String)>,
    pub run_on_start: bool,
    pub select_new_app: bool,
    pub volume_scroll_size: u8,
    pub invert_volume: bool,
    pub invert_navigation: bool,
    pub add_to_start: bool,
    pub notifications: NotificationConfig,
    pub display: DisplayConfig,
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
                    error!("[Config] Failed to deserialize config.toml:\n{}", e);
                    toml::from_str(create_default_config(true)).expect("[Config] Failed to deserialize default config")
                }
            }
        },
        None => {
            error!("[Config] Could not find config.toml");
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
        Ok(()) => { info!("[Config] Created default config file at {}", file_name); },
        Err(e) => { info!("[Config] Failed to create default config file: {}", e); }
    }
    default_config
}


// Functions for sending config to microcontroller
impl DisplayConfig {
    // Convert (24bit) hex colors to rgb565
    fn to_rgb565(color: u64) -> u16 {
        // Extract color channels
        let r = ((color >> 16) & 0xFF) as u8;
        let g = ((color >> 8) & 0xFF) as u8;
        let b = (color & 0xFF) as u8;

        // Scale down to 565
        let r5 = (r >> 3) as u16;
        let g6 = (g >> 2) as u16;
        let b5 = (b >> 3) as u16;

        // Pack into u16
        (r5 << 11) | (g6 << 5) | b5
    }

    pub fn to_packet(&self) -> crate::serial::Packet {
        let mut bytes = [0u8; CONFIG_SIZE];

        // set header
        bytes[0] = crate::serial::CONFIG_HEADER;

        // invert byte - used to set display rotation
        bytes[1] = self.invert as u8;

        // Convert colors and load into bytes
        DisplayConfig::to_rgb565(self.text_color).to_le_bytes().into_iter()
        .enumerate().for_each( |(i, byte)| bytes[i+2] = byte );
        DisplayConfig::to_rgb565(self.background_color).to_le_bytes().into_iter()
        .enumerate().for_each( |(i, byte)| bytes[i+4] = byte );

        crate::serial::Packet::Config(bytes)
    }
}
