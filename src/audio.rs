use log::{debug, error};
use crossbeam_channel::{Sender, Receiver, select};
use std::collections::BTreeMap;
use std::path::Path;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;
use windows::Win32::Foundation::{BOOL, CloseHandle, MAX_PATH};
use windows::core::{Interface, GUID, PWSTR, PCWSTR};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoW, GetFileVersionInfoSizeW, VerQueryValueW};

use crate::config;

// Helper function to resolve a PID to its executable name
fn get_process_name(pid: u32) -> String {
    unsafe {
        // Get the exe path from the pid
        // Open process handle
        let process_handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                Ok(h) => h,
                Err(e) => {
                    error!("[Audio Subsystem] Error opening process handle for PID {}: {:?}", pid, e);
                    return "Unknown".to_string()
            }
            };

        // Buffer for path
        let mut path_buffer = [0u16; MAX_PATH as usize];
        let mut path_size = path_buffer.len() as u32;

        // Querry path, path will be placed into path_buffer
        let result = QueryFullProcessImageNameW(
            process_handle,
            PROCESS_NAME_WIN32, // Specifies format as C:\... style path
            PWSTR::from_raw(path_buffer.as_mut_ptr()), // Set to buffer to path
            &mut path_size // Set to size of path
        );

        // Close handle and unwrap
        let _ = CloseHandle(process_handle);
        let exe_path = match result {
                Ok(_) => String::from_utf16_lossy(&path_buffer[..path_size as usize]),
                Err(e) => {
                    error!("[Audio Subsystem] Error querying process image: {:?}", e);
                    return "Unknown".to_string()
            }
            };

        // Set backup name from path
        let mut name = match Path::new(&exe_path).file_stem(){
            Some(os_str) => String::from(os_str.to_str().unwrap_or("Unknown")),
            None => "Unknown".to_string()
        }; 

        // Get display name from path 

        // Convert path to wide windows path
        let path_wide: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();
        let mut zero = 0; // Unused but requred by GetFileVersionInfoSizeW (legacy)

        // Get the size of the version info (need to alocate buffer)
        let info_size = GetFileVersionInfoSizeW(PCWSTR::from_raw(path_wide.as_ptr()), Some(&mut zero));
        if info_size == 0 {
            // 0 size represents an error
            error!("[Audio Subsystem] Error getting file info size, will return executable filename: {}", name);
            return name; 
        } 

        // 2. Allocate buffer and get info
        let mut info_buffer = vec![0u8; info_size as usize];
        if GetFileVersionInfoW(PCWSTR::from_raw(path_wide.as_ptr()), 0, info_size, info_buffer.as_mut_ptr() as *mut _).is_ok() {
            // If file version info was fetched sucessfully
            // Query the "FileDescription" translation block ; 040904B0 is english
            // TODO: Query translation list first for non-english languages
            let subblock = "\\StringFileInfo\\040904B0\\FileDescription\0".encode_utf16().collect::<Vec<u16>>();
            let mut value_ptr = std::ptr::null_mut();
            let mut len = 0;

            if VerQueryValueW(info_buffer.as_ptr() as *const _, PCWSTR::from_raw(subblock.as_ptr()), &mut value_ptr, &mut len).as_bool() {
                name = String::from_utf16_lossy( std::slice::from_raw_parts(value_ptr as *const u16, len as usize - 1) );
            }
        }
        return name
    }
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
        if self.blacklist.contains(&name) {
            debug!("Ignoring blacklisted application: {}", name);
            return Ok(());
        }

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