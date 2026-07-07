#![allow(unused)]

use std::thread;
use crossbeam_channel::{unbounded, select};
use log::{info, debug, error};
use env_logger::Builder;

mod config;
mod audio;


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


    // Spawn threads
    debug!("[Coordinator] Spawning threads");
    // Audio thread
    let audio_handle = thread::spawn(move || { audio::run_audio_subsystem(audio_tx); });

    // Main Loop
    loop {
        select! {
            recv(audio_rx) -> response => {
                match response{
                    Ok(message) => {
                        println!("Message type: {}", std::any::type_name_of_val(&message));
                        println!("Message: {:?}", message);
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