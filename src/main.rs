#![allow(unused)]

use std::thread;
use crossbeam_channel::{unbounded, select};
use log::{info, debug, error};
use env_logger::Builder;
use std::collections::BTreeMap;

mod config;
mod audio;
mod serial;

// Holds all needed info for a single application
struct VolumeStatus {
    volume: u8,
    muted: bool,
    name: String,
}

struct MixerStatus {
    apps: BTreeMap<u32, VolumeStatus>
}


fn main() {
    // Init global config
    config::init();
    let cfg = config::get();
    // Set up logger
    let log_level = if cfg.verbose {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    Builder::new().filter_level(log_level).init();

    debug!("{:?}", cfg);


    // Setup crossbeam channels
    // Channel for Audio thread to talk to Coordinator
    let (audio_tx, audio_rx) = crossbeam_channel::unbounded::<audio::AudioMsg>();

    // Channels for reading and writing for serial threads
    let (serial_read_tx, serial_read_rx) = crossbeam_channel::unbounded::<serial::ControlMsg>();
    let (serial_write_tx, serial_write_rx) = crossbeam_channel::unbounded::<serial::ControlMsg>();


    // Spawn threads
    debug!("[Coordinator] Spawning threads");
    // Audio thread
    let audio_handle = thread::spawn(move || { audio::run_audio_subsystem(audio_tx); });
    let serial_handle = thread::spawn(move || { serial::run_serial_subsystem(serial_read_tx, serial_write_rx) });

    // Main Loop
    loop {
        select! {
            recv(audio_rx) -> response => {
                match response{
                    Ok(message) => {
                        debug!("[Coordinator] Audio message: {:?}", message);
                    }
                    Err(_) => {
                        error!("[Coordinator] Audio thread disconnected! Breaking coordinator loop.");
                        break; 
                    }
                }
                
            }
        }

        
    }
    // Join threads if loop is exited
    audio_handle.join().unwrap();

}