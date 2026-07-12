use log::debug;
use crossbeam_channel::{Sender, Receiver, select};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;
use windows::Win32::Foundation::BOOL;
use windows::core::{Interface, GUID};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS
};

use crate::config;

// Helper function to resolve a PID to its executable name
fn get_process_name(pid: u32) -> String {
    unsafe {
        // Take a snapshot of all concurrent processes in the system
        let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(handle) => handle,
            Err(_) => return "Unknown".to_string(),
        };

        let mut entry = PROCESSENTRY32W::default();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        // Start iterating through the snapshot
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                if entry.th32ProcessID == pid {
                    // Find the end of the null-terminated wide string (u16)
                    let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                    // Convert the wide string array to a standard Rust String
                    let os_str = OsString::from_wide(&entry.szExeFile[..len]);
                    let name = os_str.to_string_lossy().into_owned();
                    return match name.rsplit_once('.') {
                        Some((before, _after)) => before.to_string(),
                        None => name, // Return original if no dot exists
                    };
                }
                
                // Move to the next process in the snapshot
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
    }
    "Unknown".to_string()
}

// External messages (sent to coordinator)
#[derive(Debug, Clone)]
pub enum AudioMsg {
    AppOpened { pid: u32, name: String, volume: f32, muted: bool }, // called both during initiliazation and when apps opened while running
    VolumeChanged { pid: u32, volume: f32, muted: bool },
    AppClosed { pid: u32 },
}

#[derive(Debug)]
pub enum SetAudio {
    Volume {pid: u32, to: f32},
    Mute {pid: u32, on: bool}
}


// Internal messages (Used localy)
enum InternalAudioMsg {
    AppOpenedIncoming(IAudioSessionControl),
    VolumeChanged { pid: u32, volume: f32, muted: bool },
    AppClosed { pid: u32 },
}


// COM LISTENERS 
#[windows::core::implement(IAudioSessionNotification)]
struct GlobalSessionListener {
    tx: Sender<InternalAudioMsg>,
}

impl IAudioSessionNotification_Impl for GlobalSessionListener_Impl {
    fn OnSessionCreated(&self, newsession: Option<&IAudioSessionControl>) -> windows::core::Result<()> {
        if let Some(session) = newsession {
            // Pass the raw COM object into our thread's loop for safe processing
            let _ = self.tx.send(InternalAudioMsg::AppOpenedIncoming(session.clone()));
        }
        Ok(())
    }
}

#[windows::core::implement(IAudioSessionEvents)]
struct AppEventListener {
    pid: u32,
    tx: Sender<InternalAudioMsg>,
}

impl IAudioSessionEvents_Impl for AppEventListener_Impl {
    fn OnSimpleVolumeChanged(&self, newvolume: f32, is_muted: BOOL, _context: *const windows::core::GUID) -> windows::core::Result<()> {
        // Called when user manually adjusts windows mixer
        let _ = self.tx.send(InternalAudioMsg::VolumeChanged { pid: self.pid, volume: newvolume, muted: is_muted.as_bool() });
        Ok(())
    }
    fn OnSessionDisconnected(&self, _reason: AudioSessionDisconnectReason) -> windows::core::Result<()> {
        // Almost never called (oops)
        let _ = self.tx.send(InternalAudioMsg::AppClosed { pid: self.pid });
        Ok(())
    }
    fn OnStateChanged(&self, newstate: AudioSessionState) -> windows::core::Result<()> {
        // Called when user applications close or crash
        if newstate == AudioSessionStateExpired {
            let _ = self.tx.send(InternalAudioMsg::AppClosed { pid: self.pid });
        }
        Ok(())
    }
    
    // Unused events
    fn OnChannelVolumeChanged(&self, _: u32, _: *const f32, _: u32, _: *const windows::core::GUID) -> windows::core::Result<()> { Ok(()) }
    fn OnDisplayNameChanged(&self, _: &windows::core::PCWSTR, _: *const windows::core::GUID) -> windows::core::Result<()> { Ok(()) }
    fn OnIconPathChanged(&self, _: &windows::core::PCWSTR, _: *const windows::core::GUID) -> windows::core::Result<()> { Ok(()) }
    fn OnGroupingParamChanged(&self, _: *const windows::core::GUID, _: *const windows::core::GUID) -> windows::core::Result<()> { Ok(()) }
    
}

// ==========================================
// STATE MANAGEMENT 
// ==========================================
struct TrackedSession {
    pub _pid: u32,
    pub _name: String,
    pub control: IAudioSessionControl,
    _listener: IAudioSessionEvents, // Keeps listener alive in memory (for windows nonsense)
}

struct AudioStateManager {
    sessions: BTreeMap<u32, TrackedSession>,
    to_coordinator: Sender<AudioMsg>,
    blacklist: Vec<String>,
}

impl AudioStateManager {
    pub fn new(to_coordinator: Sender<AudioMsg>) -> Self {
        let blacklist = config::get().blacklist.clone(); // load blacklist
        Self { sessions: BTreeMap::new(), to_coordinator, blacklist }
    }

    pub fn add_session(&mut self, session: IAudioSessionControl, internal_tx: Sender<InternalAudioMsg>) -> windows::core::Result<()> {
        let session2: IAudioSessionControl2 = session.cast()?;
        let pid = unsafe { session2.GetProcessId()? };

        // Return if pid == 0; don't care about the system
        if pid == 0 { return Ok(()) }
        let name = get_process_name(pid);
        if name == "Unknown" { return Ok(()) } // Ignore apps with unknown names, usually don't want them

        // check name against blacklist
        if self.blacklist.contains(&name) {return Ok(());}

        // prevent repeated entries
        if self.sessions.contains_key(&pid) { return Ok(()); }

        let app_listener: IAudioSessionEvents = AppEventListener { pid, tx: internal_tx }.into();
        unsafe { session.RegisterAudioSessionNotification(&app_listener)?; }

        // Get volume
        let volume_control: ISimpleAudioVolume = session.cast()?;
        let volume = unsafe { volume_control.GetMasterVolume()? };
        let mute = unsafe { volume_control.GetMute()? };

        self.sessions.insert(pid, TrackedSession {
            _pid: pid,
            _name: name.clone(),
            control: session,
            _listener: app_listener,
        });

        let _ = self.to_coordinator.send(AudioMsg::AppOpened { pid, name, volume, muted: mute.as_bool() });
        Ok(())
    }

    pub fn remove_session(&mut self, pid: u32) {
        if let Some(removed) = self.sessions.remove(&pid) {
            unsafe { let _ = removed.control.UnregisterAudioSessionNotification(&removed._listener); }
            let _ = self.to_coordinator.send(AudioMsg::AppClosed { pid });
        }
    }

    pub fn change_volume(&self, command: SetAudio)-> windows::core::Result<()> {
        debug!("[Audio Subsystem] Controling volume: {:?}", command);
        let context_guid = GUID::zeroed();
        match command {
            SetAudio::Volume {pid, to} => {
                let Some(session) = self.sessions.get(&pid) else {return Ok(())};
                let control: ISimpleAudioVolume = session.control.cast()?;
                unsafe { control.SetMasterVolume(to, &context_guid) }
            }
            SetAudio::Mute {pid, on} => {
                let Some(session) = self.sessions.get(&pid) else {return Ok(())};
                let control: ISimpleAudioVolume = session.control.cast()?;
                unsafe { control.SetMute(on, &context_guid) }
            }
        }
    }
}

// ==========================================
// THE MAIN THREAD LOOP
// ==========================================
pub fn run_audio_subsystem(to_coordinator: Sender<AudioMsg>, from_coordinator: Receiver<SetAudio>) -> windows::core::Result<()> {

    debug!("[Audio Subsystem] Initializing...");

    let (internal_tx, internal_rx) = crossbeam_channel::unbounded::<InternalAudioMsg>();
    let mut manager = AudioStateManager::new(to_coordinator.clone());

    // Initialize COM
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()?; }
    let enumerator: IMMDeviceEnumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)? };
    let asm: IAudioSessionManager2 = unsafe { device.Activate(CLSCTX_ALL, None)? };

    // Register Global Listener
    let global_listener: IAudioSessionNotification = GlobalSessionListener { tx: internal_tx.clone() }.into();
    unsafe { asm.RegisterSessionNotification(&global_listener)?; }

    // Currently running apps
    let session_enum = unsafe { asm.GetSessionEnumerator()? };
    for i in 0..unsafe { session_enum.GetCount()? } {
        if let Ok(session) = unsafe { session_enum.GetSession(i) } {
            let _ = manager.add_session(session, internal_tx.clone());
        }
    }

    debug!("[Audio Subsystem] Initialization complete.");

    // Main Loop (Serializes all asynchronous COM callbacks)
    loop {
        select!{
            // internal messages containing audio events
            recv(internal_rx) -> msg => {
                match msg.expect("[Audio Subsystem] Critical error reading message from OS listener.") {
                    InternalAudioMsg::AppOpenedIncoming(session) => {
                        let _ = manager.add_session(session, internal_tx.clone());
                    }
                    InternalAudioMsg::AppClosed { pid } => manager.remove_session(pid),
                    InternalAudioMsg::VolumeChanged { pid, volume, muted } => {
                        // send message directly bc manager does not track volume directly
                        let _ = manager.to_coordinator.send(AudioMsg::VolumeChanged { pid, volume, muted }); 
                    }
                }
            }
            // external messages containing commands
            recv(from_coordinator) -> msg => {
                let _ = manager.change_volume(msg.expect("[Audio Subsystem] Critical error reading message from coordinator."));
            }
        }
    }
}