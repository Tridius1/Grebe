use std::time::Instant;
use std::process::Command;

use viuer::{print, Config};
use image::{ImageBuffer, Rgba};

use serialport;

use std::collections::BTreeMap;

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
    Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, SendMessageW, GCL_HICON, GetClassLongPtrW, ICON_SMALL2, WM_GETICON, HICON, GetIconInfo,
    },
    Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, 
    BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC, HBITMAP
    },
    Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_NAME_WIN32
    },
    Win32::Foundation::{HWND, LPARAM, WPARAM, CloseHandle},
    Win32::UI::Shell::ExtractIconW,
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


#[derive(Debug)]
pub struct IconPixels {
    pub width: i32,
    pub height: i32,
    pub bgra_bytes: Vec<u8>,
}

/// Takes a valid HICON and extracts its raw 32-bit BGRA bitmap pixels using GDI.
pub fn bytes_from_hicon(opt_hicon: Option<HICON>) -> Option<IconPixels> {

    let hicon = opt_hicon?;

    unsafe {
        // 1. Extract structural icon info (brings along color & mask HBITMAPs)
        let mut icon_info = std::mem::zeroed();
        if !GetIconInfo(hicon, &mut icon_info).is_ok() {
            return None;
        }

        // Defer deletion of the internal bitmaps to prevent memory leaks
        let _color_guard = ScopeGuard(icon_info.hbmColor);
        let _mask_guard = ScopeGuard(icon_info.hbmMask);

        if icon_info.hbmColor.is_invalid() {
            return None;
        }

        // 2. Query the color bitmap to figure out its actual dimensions (Width x Height)
        let mut bitmap: BITMAP = std::mem::zeroed();
        let size_of_bitmap = std::mem::size_of::<BITMAP>() as i32;
        
        let bytes_read = GetObjectW(
            icon_info.hbmColor,
            size_of_bitmap,
            Some(&mut bitmap as *mut BITMAP as *mut _),
        );

        if bytes_read != size_of_bitmap {
            return None;
        }

        let width = bitmap.bmWidth;
        let height = bitmap.bmHeight;

        // 3. Set up a Device Context (DC) to allow GDI to read pixel data
        let hdc_mem = CreateCompatibleDC(HDC(0));
        if hdc_mem.is_invalid() {
            return None;
        }

        // 4. Construct the Target Layout (32-bit BGRA, Uncompressed)
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // Negative height for top-down orientation
                biPlanes: 1,
                biBitCount: 32,    // 4 bytes per pixel: B, G, R, A
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        // 5. Allocate memory buffer (Width * Height * 4 bytes per pixel)
        let total_pixels = (width * height) as usize;
        let mut bgra_bytes = vec![0u8; total_pixels * 4];

        // 6. Dump the bits from the handle into our Rust vector
        let scanlines_copied = GetDIBits(
            hdc_mem,
            icon_info.hbmColor,
            0,
            height as u32,
            Some(bgra_bytes.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        let _ = DeleteDC(hdc_mem); // Clean up the memory device context

        if scanlines_copied == 0 {
            return None;
        }

        Some(IconPixels {
            width,
            height,
            bgra_bytes,
        })
    }
}

/// A simple RAII pattern helper to ensure underlying HBITMAP handles are dropped cleanly
struct ScopeGuard(HBITMAP);
impl Drop for ScopeGuard {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe { 
                let _ = DeleteObject(self.0); 
            }
        }
    }
}



//--------------------------------------------------------------------------------------
// PRINTING ICONS TO DEBUG
pub fn display_icon_in_terminal(icon_data: &IconPixels) {
    // GDI gives us BGRA, but the image crate and terminals expect RGBA
    // Copy and swap the channels
    let mut rgba_bytes = icon_data.bgra_bytes.clone();
    for chunk in rgba_bytes.chunks_exact_mut(4) {
        chunk.swap(0, 2); // Swap Blue (0) and Red (2) -> RGBA
    }

    // Convert raw bytes into an ImageBuffer from the image crate
    let width = icon_data.width as u32;
    let height = icon_data.height as u32;
    
    let img_buffer: ImageBuffer<Rgba<u8>, Vec<u8>> = 
        ImageBuffer::from_raw(width, height, rgba_bytes)
            .expect("Failed to create image buffer from icon bytes");

    // Convert the buffer into a dynamic image type that viuer uses
    let dynamic_img = image::DynamicImage::ImageRgba8(img_buffer);

    // Configure viuer options for terminal printing
    let config = Config {
        // Limit the width in the terminal so a massive icon doesn't blow up your layout
        width: Some(32), 
        height: Some(16),
        absolute_offset: false,
        transparent: true, // Respect the application icon's transparency alpha layer
        use_iterm: false,
        use_kitty: false,
        ..Default::default()
    };

    println!("\n--- Debugging Icon Output ({width}x{height}) ---");
    // Print
    if let Err(e) = print(&dynamic_img, &config) {
        println!("Failed to render image in terminal: {}", e);
    }
    println!("---------------------------------------\n");
}








// A private state struct to pass data through the Win32 callback boundary
struct WindowSearch {
    target_pid: u32,
    found_hwnd: Option<HWND>,
}

/// Takes a Process ID (PID) and attempts to find its top-level window 
/// to retrieve its taskbar-equivalent HICON.
pub fn get_icon_from_pid(pid: u32) -> Option<HICON> {
    let mut search = WindowSearch {
        target_pid: pid,
        found_hwnd: None,
    };

    // 1. Find the window handle
    unsafe {
        let _ = EnumWindows(
            Some(enum_window_callback),
            LPARAM(&mut search as *mut WindowSearch as isize),
        );
    }

    // Even if we don't find an HWND, we can still try to fallback directly 
    // to the executable file by PID!
    if let Some(hwnd) = search.found_hwnd {
        unsafe {
            // Strategy A: Ask the window for its "Large" icon
            //let mut result = SendMessageW(hwnd, WM_GETICON, WPARAM(ICON_BIG as usize), LPARAM(0));
            //if result.0 != 0 { return Some(HICON(result.0)); }

            // Strategy B: Fallback to the "Small" icon 
            let result = SendMessageW(hwnd, WM_GETICON, WPARAM(ICON_SMALL2 as usize), LPARAM(0));
            if result.0 != 0 { return Some(HICON(result.0)); }

            // Strategy C: Read the window class fallback icon
            let class_icon = GetClassLongPtrW(hwnd, GCL_HICON);
            if class_icon != 0 { return Some(HICON(class_icon as isize)); }
        }
    }

    // Strategy D: CRITICAL FALLBACK (for Electron apps; Discord/Spotify)
    // Dig out the executable path from the PID and extract its embedded icon asset.
    get_icon_from_exe_path(pid)
}

/// Helper function to open a process, get its file path, and extract its icon resource
fn get_icon_from_exe_path(pid: u32) -> Option<HICON> {
    unsafe {
        // Open a handle to the process
        let process_handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        
        // Query the full path to the .exe file (e.g. "C:\Users\...\Discord.exe")
        let path_success = QueryFullProcessImageNameW(
            process_handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut size
        ).is_ok();
        
        let _ = CloseHandle(process_handle); // Clean up the process handle
        
        if !path_success {
            return None;
        }

        // ExtractIconW looks for the first icon (index 0) baked into the executable
        let path_pwstr = windows::core::PCWSTR(buffer.as_ptr());
        let extracted_hicon = ExtractIconW(None, path_pwstr, 0);
        
        // ExtractIconW returns 0, 1, or an invalid handle value on failure.
        // It returns a handle cast as an isize on success.
        if extracted_hicon.0 == 0 || extracted_hicon.0 == 1 {
            None
        } else {
            Some(HICON(extracted_hicon.0))
        }
    }
}

// The Win32 callback used by EnumWindows
unsafe extern "system" fn enum_window_callback(hwnd: HWND, lparam: LPARAM) -> windows::Win32::Foundation::BOOL { unsafe {
    let search = &mut *(lparam.0 as *mut WindowSearch);
    let mut process_id = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut process_id));

    if process_id == search.target_pid {
        search.found_hwnd = Some(hwnd);
        return false.into(); 
    }
    true.into() 
}}


struct VolumeSession {
    name: String,
    icon: Option<IconPixels>,
}




fn check_mixer(volumes: &mut BTreeMap<u32, VolumeSession>) -> Result<()>  {
    unsafe {
        // Initialize COM library for the current thread
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;

        // Get the device enumerator (CoCreateInstance comes from System::Com)
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

        // Get the default audio output device
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;

        // Get the IAudioSessionManager2 interface from the device
        let session_manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;

        // Get the session enumerator
        let session_enumerator = session_manager.GetSessionEnumerator()?;
        let session_count = session_enumerator.GetCount()?;

        // Vector to check each memoized pid is still relevant
        let mut extra_pids: Vec<_> = volumes.keys().cloned().collect();
        
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

            // Check if process is cached, use cache if so otherwise fill cache
            if  !volumes.contains_key(&pid) {
                volumes.insert(pid, VolumeSession{
                    name: get_process_name(pid),
                    icon: bytes_from_hicon(get_icon_from_pid(pid))
                });
            } else {
                // if the volume is cached it is still relevant
                let pid_pos = extra_pids.iter().position(|&x| x == pid);
                let _ = match pid_pos {
                    Some(pid_pos) => extra_pids.remove(pid_pos),
                    None => 0,
                };
            }
            let vol_session = volumes.get(&pid).unwrap();


            let has_hicon = if vol_session.icon.is_some() {"Has HICON"} else {"No HICON"};

            if is_muted {
                println!("{:<10} MUTED {:<80} | {}", pid, vol_session.name, has_hicon);
            } else {
                println!("{:<10} {:.0}% {:<80} | {}", pid, volume * 100.0, vol_session.name, has_hicon);
            }
            //if vol_session.icon.is_some(){
            //    display_icon_in_terminal( vol_session.icon.as_ref().unwrap() );
            //}
            
        }
        // Remove extra PIDs from BTreeMap
        extra_pids.iter().for_each(|pid| { volumes.remove(pid); })
    }
    Ok(())
}



// Serial Interaction ###############################################

fn check_connection() -> Result<()> {
    // Open serial port
    let mut port = serialport::new("COM3", 115_200);

    return Ok(())
}




fn main() -> Result<()> {

    // BTree to hold process info and icons
    let mut volumes: BTreeMap<u32, VolumeSession> = BTreeMap::new();

    for _ in 1..10{
        let start = Instant::now();
        let _ = check_mixer(&mut volumes);

        
        let duration = start.elapsed();
        println!("Execution time: {:?}", duration);


        let _ = Command::new("cmd.exe").arg("/c").arg("pause").status();
    }
    

    Ok(())
}