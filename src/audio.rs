use env_logger::Builder;
use log::{info, debug, error};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use windows::{
    core::{Result, Interface},
    Win32::Media::Audio::{
        IMMDeviceEnumerator, MMDeviceEnumerator, eRender, eConsole, 
        IAudioSessionManager2, IAudioSessionControl2, ISimpleAudioVolume
    },
    Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, COINIT_MULTITHREADED, CLSCTX_ALL
    },
    Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS
    },
};

use crate::config;


// Struct for sending new volume state to controller
#[derive(Debug)]
pub struct VolumeState {
    pid: u32,
    name: Option<String>,
    volume: f32,
    muted: bool,
}

// Enum for messages to controller
#[derive(Debug)]
pub enum AudioMsg {
    New(VolumeState), // used during initilization and when new programs are opened
    Set(VolumeState), // used when volume changes
    Del(u32), // used when program is closed; contains a PID
}





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
                    // Remove file extention if exists
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

    // Fallback to "Unknown"
    "Unknown".to_string()
}





pub fn listener(controller_tx: crossbeam_channel::Sender<AudioMsg>) -> Result<()>  {
    // Get global config
    let cfg = config::get();
    
    debug!("[Audio Thread] Thread initialized");

    unsafe {
        // Initialize COM library for the current thread
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;

        // Get the device enumerator (CoCreateInstance comes from System::Com)
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

        // Get the default audio output device
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;

        // Get the IAudioSessionManager2 interface from the device
        let session_manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;


        // #### Get all open sessions and their current volume to initialize ####
        // Get the session enumerator
        let session_enumerator = session_manager.GetSessionEnumerator()?;
        let session_count = session_enumerator.GetCount()?;
        debug!("[Audio Thread] Opened session enumerator, session count = {}", session_count);

        // Loop through each audio session
        for i in 0..session_count {
            let session_control = session_enumerator.GetSession(i)?;
            
            // Cast to IAudioSessionControl2 to get Process information
            let session_control2: IAudioSessionControl2 = session_control.cast()?;
            let pid = session_control2.GetProcessId()?;

            // Skip the system sounds session if it doesn't belong to a specific application PID
            if pid == 0 {
                continue; 
            }

            // Cast to ISimpleAudioVolume to get volume levels
            let audio_volume: ISimpleAudioVolume = session_control2.cast()?;
            let volume = audio_volume.GetMasterVolume()?;
            let is_muted = audio_volume.GetMute()?.as_bool();

            // Send controller new VolumeState
            let vol_msg =AudioMsg::New( VolumeState {
                pid: pid,
                name: Some(get_process_name(pid)),
                volume: volume,
                muted: is_muted,
            });

            // Get rid of Result *** TODO: Handle Error here ***
            let _ = controller_tx.send(vol_msg);            
        }
        debug!("[Audio Thread] Audio listener initialization complete.");
    }
    // TODO: Event Listening



    Ok(())
}
