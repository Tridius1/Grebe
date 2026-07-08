#![allow(unused)]

//#![windows_subsystem = "windows"] // Uncomment this to hide the console

use std::error::Error;
use winit::{
    event_loop::{ControlFlow, EventLoop, ActiveEventLoop},
    event::WindowEvent,
    window::WindowId,
    application::ApplicationHandler
};
use tray_icon::{
    menu::{Menu, MenuId, MenuEvent, MenuItem},
    Icon, TrayIcon, TrayIconBuilder, TrayIconEvent
};
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

// The prime control loop
fn coordinator() {
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

// Creates a windows icon from png
fn get_icon() -> Icon {
    // include_bytes! embeds the image .exe at compile time
    let icon_bytes = include_bytes!("../grebe_icon.png");

    // Decode into RGBA
    let image = image::load_from_memory(icon_bytes)
        .expect("Failed to load icon image")
        .into_rgba8();

    // Extract the dimensions and raw pixels
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();

    // Build the tray icon
    Icon::from_rgba(rgba, width, height)
        .expect("Failed to construct tray icon")
}


// used to catch system tray interactions
enum UserEvent {
    Tray(TrayIconEvent),
    Menu(MenuEvent),
}

// Handles the way this app interacts with windows user interface
struct App {
    tray_icon: TrayIcon,
    quit_id: MenuId,
}

impl ApplicationHandler<UserEvent> for App {
    // REQUIRED: Called when the app is resumed (mostly for mobile/web).
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    // REQUIRED: Called for window events. We have no GUI window, so we do nothing.
    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {}

    // OPTIONAL: But crucial for us! This handles our custom Tray & Menu proxy events.
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Menu(menu_event) => {
                // Check if the Exit button was clicked
                if menu_event.id == self.quit_id {
                    debug!("[ApplicationHandler] System tray exit was clicked. Exiting.");
                    // Hard shutdown; TODO: Send shutdown msg to threads, join and gracefully shutdown
                    event_loop.exit();
                }
            }
            UserEvent::Tray(tray_event) => {
                // Do nothing, this is called whenever the tray icon is moused over
            }
        }
    }
}


// Runs windows event loop
fn main() -> Result<(), Box<dyn Error>> {
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

    // Spawn coordinator, it does all the important work
    thread::spawn( || { coordinator() } );

    // Set up event loop 
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    // Setup Event Proxies
    let proxy = event_loop.create_proxy();
    let proxy_menu = proxy.clone();

    TrayIconEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::Tray(event));
    }));
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy_menu.send_event(UserEvent::Menu(event));
    }));

    // Setup the Context Menu
    let tray_menu = Menu::new();
    let quit_item = MenuItem::new("Exit", true, None);
    
    // Store the ID so we can match it inside the trait later
    let quit_id = quit_item.id().clone();
    
    tray_menu.append(&quit_item)?;

    let mut tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Grebe Audio Mixer")
            .with_icon(get_icon()) 
            .build()?;


    // Instantiate App and associated event hooks
    let mut app = App {
        tray_icon: tray_icon,
        quit_id,
    };
    
    // Run the event loop and listen for user interactons
    event_loop.run_app(&mut app)?;

    Ok(())
}