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
use env_logger::{Builder, Target, WriteStyle};
use std::io::{stderr, IsTerminal};
use std::fs::File;
use std::collections::BTreeMap;

mod config;
mod audio;
mod serial;

const MAX_NAME_LEN: usize = 20; // size of char array that will be sent to the arduino; arduino expects 20
const ENTRY_SIZE: usize = (MAX_NAME_LEN + 2); //size of FrameEntry in bytes; name + volume + mute
const FRAME_SIZE: usize = (ENTRY_SIZE * 3) + 1; //size of DisplayFrame in bytes; 3 entries + 1 header byte

// Holds all needed info for a single application
#[derive(Debug)]
struct VolumeStatus {
    volume: u8,
    muted: bool,
    name: String,
}

// Manages the state of the mixer, all apps and their data
#[derive(Debug)]
struct MixerManager {
    apps: BTreeMap<u32, VolumeStatus>,
    count: usize,
    selected_index: usize
}

impl MixerManager {
    pub fn new() -> Self {
        Self {apps: BTreeMap::new(), count: 0, selected_index: 0}
    }

    // Called when user spins nav knob
    pub fn scroll(&mut self, is_up: bool) {
        if is_up { self.selected_index = (self.selected_index + 1) % (self.count as usize) }
        else { self.selected_index = ((self.count + self.selected_index) - 1) % (self.count as usize) }

        //debug
        let keys: Vec<u32> = self.apps.keys().cloned().collect();
        let selected_key = keys[self.selected_index];
        debug!("Scrolled to: {:?}", self.apps[&selected_key]);
    }


    pub fn audio_update(&mut self, message: audio::AudioMsg) {
        match message {
            audio::AudioMsg::AppOpened {pid, name, volume: f_volume, muted} => {
                self.apps.insert(pid, VolumeStatus{
                    name,
                    volume: MixerManager::convert_volume(f_volume),
                    muted
                });
                self.count += 1;
                // select the new app
                if config::get().select_new_app {
                    let keys: Vec<u32> = self.apps.keys().cloned().collect();
                    self.selected_index = keys.iter().position( |&k| k == pid ).expect("[Coordinator] Cannot find new pid in keys.");
                    let selected_key = keys[self.selected_index];
                    debug!("Scrolled to new app: {:?}", self.apps[&selected_key]);
                }
            }
            audio::AudioMsg::VolumeChanged {pid, volume, muted} => {
                if let Some(status) = self.apps.get_mut(&pid) {
                    status.volume = MixerManager::convert_volume(volume);
                    status.muted = muted;
                }
            }
            audio::AudioMsg::AppClosed {pid} => {
                let _ = self.apps.remove(&pid);
                self.count -= 1;
                // Ensure closing app is not selected
                if self.selected_index >= self.apps.keys().count() { self.selected_index -= 1; }
            }
        }
    }

    fn convert_volume(f_volume: f32) -> u8 {
        (100.0 * f_volume).round() as u8
    }

    pub fn frame(&self) -> DisplayFrame {
        if self.count == 0 { // easy out if no apps open
            return DisplayFrame::new(None, None, None)
        }
        // manage index -> key (pid)
        let keys: Vec<u32> = self.apps.keys().cloned().collect();
        let selected_key = keys[self.selected_index];
        if self.count == 1 {
            return DisplayFrame::new(None, self.apps.get(&selected_key), None)
        }
        // 2 is tricky, we want to maintain order on screen instead of wrapping
        if self.count == 2 {
            if self.selected_index == 0 {
                // [1] is always next if not selected
                return DisplayFrame::new(None, self.apps.get(&selected_key), self.apps.get(&keys[1]))
            }
            else {
                // [0] is always prev if not selected
                return DisplayFrame::new(self.apps.get(&keys[0]), self.apps.get(&selected_key), None)
            }
        }
        // Generic option, 3+ apps in list; use modulo to wrap around list
        let next_key = keys[ (self.selected_index + 1) % (self.count as usize) ];
        let prev_key = keys[ ((self.count + self.selected_index) - 1) % (self.count as usize) ];
        return DisplayFrame::new(self.apps.get(&prev_key), self.apps.get(&selected_key), self.apps.get(&next_key))
    }
}

// one of three entrys in a frame
struct FrameEntry {
    pub volume: u8,
    pub muted: u8,
    pub name: [u8; MAX_NAME_LEN]
}
impl FrameEntry {
    // new FrameEntry from VolumeStatus, or empty frame if none
    fn new(status: Option<&VolumeStatus>) -> Self {
        // buffer of ASCII values, 0 to start
        let mut name_bytes = [0u8; MAX_NAME_LEN];
        match status {
            Some(status) => {
                // convert name string to array of ASCII values
                let bytes = status.name.as_bytes();
                let len = bytes.len().min(MAX_NAME_LEN - 1); // cap the length of the string
                name_bytes[..len].copy_from_slice(&bytes[..len]);

                // create entry
                Self {
                    volume: status.volume,
                    muted: status.muted as u8,
                    name: name_bytes
                }
            }
            None => {
                Self {
                    volume: 0,
                    muted: 0,
                    name: name_bytes
                }
            }
        }
        
    }
}

// All info needed to send a frame to the Arduino
pub struct DisplayFrame {
    entries: [FrameEntry; 3], // [0] = Prev, [1] = Curr, [2] = Next
}

impl DisplayFrame {
    fn new (prev: Option<&VolumeStatus>, curr: Option<&VolumeStatus>, next: Option<&VolumeStatus>) -> Self {
        if curr.is_none() {
            if prev.is_none() || next.is_none() {
                // this state represents a logic error; curr should be Some unless there are no apps
                panic!("DisplayFrame.from_VolumeStatus called with no current, but some prev or next. ");
            }
        }
        // populate frame
        Self {
            entries: [FrameEntry::new(prev), FrameEntry::new(curr), FrameEntry::new(next)]
        }
    }

    // Serialize frame to send to Arduino
    fn to_bytes(&self) -> serial::FramePacket {
        let mut bytes = [0u8; FRAME_SIZE];

        //set header
        bytes[0] = serial::FRAME_HEADER;

        // iterate over entries and populate buffer
        for (i, entry) in self.entries.iter().enumerate() {
            let start = 1 + (i * ENTRY_SIZE);
            bytes[start] = entry.volume;
            bytes[start + 1] = entry.muted;
            // name char array
            let name_start = start + 2;
            let name_end = name_start + MAX_NAME_LEN;
            bytes[name_start..name_end].copy_from_slice(&entry.name);
        }

        return serial::FramePacket(bytes)
    }
}

// The prime control loop
fn coordinator() {
    let cfg = config::get();

    // Setup crossbeam channels
    // Channel for Audio thread to talk to Coordinator
    let (audio_tx, audio_rx) = crossbeam_channel::unbounded::<audio::AudioMsg>();

    // Channels for reading and writing for serial threads
    let (serial_read_tx, serial_read_rx) = crossbeam_channel::unbounded::<serial::ControlMsg>();
    let (serial_write_tx, serial_write_rx) = crossbeam_channel::unbounded::<serial::FramePacket>();

    // Create manager
    let mut manager = MixerManager::new();

    // Spawn threads
    debug!("[Coordinator] Spawning threads");
    // Audio thread
    let audio_handle = thread::spawn(move || { audio::run_audio_subsystem(audio_tx); });
    let serial_handle = thread::spawn(move || { serial::run_serial_subsystem(serial_read_tx, serial_write_rx) });

    // Main Loop
    loop {
        select! {
            recv(audio_rx) -> received => {
                match received{
                    Ok(message) => {
                        debug!("[Coordinator] Audio message: {:?}", message);
                        manager.audio_update(message); // update the manager
                        // send new frame to arduino
                        serial_write_tx.send(manager.frame().to_bytes());
                    }
                    Err(_) => {
                        error!("[Coordinator] Audio thread disconnected! Breaking coordinator loop.");
                        break; 
                    }
                }
            }
            recv(serial_read_rx) -> received => {
                match received {
                    Ok(command) => {
                        debug!("[Coordinator] Serial command: {:?}", command);
                        match command {
                            serial::ControlMsg::AppScroll{up} => {
                                let is_up = if cfg.invert_volume {!up} else {up};
                                manager.scroll(is_up);
                                // send new frame to arduino
                                serial_write_tx.send(manager.frame().to_bytes());

                            }
                            serial::ControlMsg::VolumeScroll{up} => {
                                let is_up = if cfg.invert_volume {!up} else {up};
                            }
                            serial::ControlMsg::MuteToggle => {}
                            serial::ControlMsg::NewFrame => {}
                        }
                    }
                    Err(_) => {
                        error!("[Coordinator] Serial thread disconnected! Breaking coordinator loop.");
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
    let mut log_builder = Builder::new();
    // Check if stderr is attached
    if stderr().is_terminal() {
        // log to terminal
        log_builder.target(Target::Stderr);
    } else {
        // log to file
        log_builder.write_style(WriteStyle::Never);
        if let Ok(log_file) = File::create("grebe.log") {
            log_builder.target(Target::Pipe(Box::new(log_file)));
        } else {
            // If the log file cannot be written, discard logs
            log_builder.target(Target::Pipe(Box::new(std::io::sink())));
        }
    }
    log_builder.filter_level(log_level).init();

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