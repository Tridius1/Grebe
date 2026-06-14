use std::time::Instant;


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
                    return os_str.to_string_lossy().into_owned();
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

fn main() -> Result<()> {
    let start = Instant::now();

    unsafe {
        // Initialize COM library for the current thread
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;

        // 1. Get the device enumerator (CoCreateInstance comes from System::Com)
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

        // 2. Get the default audio output device (Speakers/Headphones)
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;

        // 3. Get the IAudioSessionManager2 interface from the device
        let session_manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;

        // 4. Get the session enumerator
        let session_enumerator = session_manager.GetSessionEnumerator()?;
        let session_count = session_enumerator.GetCount()?;

        println!("{:<10} {:<10}", "PID", "Volume");
        println!("---------------------");

        // 5. Loop through each audio session
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

            let process_name = get_process_name(pid);

            if is_muted {
                println!("{:<10} MUTED {:<80}", pid, process_name);
            } else {
                println!("{:<10} {:.0}% {:<80}", pid, volume * 100.0, process_name);
            }
        }
    }


    let duration = start.elapsed();

    println!("Execution time: {:?}", duration);
    Ok(())
}