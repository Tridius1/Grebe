use log::{error, debug};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::core::{Interface, PCWSTR};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::core::{PROPVARIANT, h, HSTRING, Result};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, IPersistFile};
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
use windows::Foundation::{DateTime, PropertyValue, IReference};

use crate::config;

// THESE MUST MATCH
pub const APP_ID: &'static str = "Grebe.HardwareVolumeMixer"; // App ID provided to Windows
pub const APP_ID_H: &'static HSTRING = h!("Grebe.HardwareVolumeMixer"); // HSTRRING App ID provided to Windows

// Send a windows notification
pub fn send_notification(title: &str, message: &str) -> Result<()> {
    debug!("[Notifications] New notification: {} : {}", title, message);
	// Abort if adding app to start menu is disabled
	if !config::get().add_to_start {
        debug!("[Notifications] Not added to start; aborting notification");
        return Ok(());
    }

    // Define the Toast template XML template string
    let xml_string = format!(
        r#"<toast>
            <visual>
                <binding template="ToastGeneric">
                    <text>{}</text>
                    <text>{}</text>
                </binding>
            </visual>
            <audio silent="{}" />
        </toast>"#,
        title, message, config::get().notifications.silent.to_string()
    );

    // Load XML string into a Windows XML Document object
    let xml_doc = XmlDocument::new()?;
    xml_doc.LoadXml(&HSTRING::from(xml_string))?;

    // Create the toast notification object from our XML layout
    let notification = ToastNotification::CreateToastNotification(&xml_doc)?;

    // Calculate experation time in windows ticks
    let nanos_since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let now_windows_ticks = (nanos_since_epoch / 100) + 116_444_736_000_000_000;
    let exp_time_ticks = now_windows_ticks as i64 + ( 10_000_000 * config::get().notifications.expiration );
    let exp_datetime = DateTime{ UniversalTime: exp_time_ticks};

    // Create Property Value for expiration time
    let dt_prop_val = PropertyValue::CreateDateTime(exp_datetime)?;
    // Cast Property Value to IReference
    let dt_iref: IReference<DateTime> = dt_prop_val.cast()?;
    // Set notification expiration
    let _ = notification.SetExpirationTime(&dt_iref);
    
    // Send the notification
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&APP_ID_H)?;
    notifier.Show(&notification)?;

    Ok(())
}

// Create start menu shortcut; needed for notifications
pub fn ensure_shortcut() {
    // Full absolute path to running exe
    let exe_path = match std::env::current_exe() {
    	Ok(path) => path,
        Err(err) => {
        	error!("[Notifications] Failed to get path to current executable: {:?}", err);
        	return;
        }
    };

    // Get path to user's start menu
    let mut shortcut_path = match std::env::var("APPDATA") {
        Ok(appdata_path) => PathBuf::from(appdata_path),
        Err(err) => {
            error!("[Notifications] Failed to read APPDATA enviroment variable: {:?}", err);
            return;
        }
    };
    shortcut_path.push(r#"Microsoft\Windows\Start Menu\Programs"#);

    // Append shortcut filename to shortcut path
    shortcut_path.push("Grebe.lnk");

    if shortcut_path.exists() {
    	debug!("[Notifications] Shortcut already exists in Start Menu\\Programs. Removing to ensure sortcut is correct.");
        // This is needed in the case that the Grebe exe was moved
    	let _ = std::fs::remove_file(&shortcut_path);
    }

    // Convert strings to windows style wide strings
    let exe_wide = to_widestr(exe_path.to_str().unwrap_or(""));
    let save_path_wide = to_widestr(shortcut_path.to_str().unwrap_or(""));

    unsafe {
    	// Initialize COM library threads 
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        // Instantiate a Windows ShellLink COM object
        let shell_link: IShellLinkW =  match CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) {
        	Ok(sh_lnk) => sh_lnk,
	        Err(err) => {
	        	error!("[Notifications] Failed create IShellLink: {:?}", err);
	        	return;
	        }
        };
        
        // Set shortcut path
        if let Err(err) = shell_link.SetPath(PCWSTR::from_raw(exe_wide.as_ptr())) {
	        error!("[Notifications] Failed set shortcut path: {:?}", err);
	        return;
	    }

	    let property_store: IPropertyStore = match shell_link.cast() {
        	Ok(sh_lnk) => sh_lnk,
	        Err(err) => {
	        	error!("[Notifications] Failed cast IShellLink to IPropertyStore: {:?}", err);
	        	return;
	        }
        };
        
        // Create the PropVariant containing the App ID
		let prop_variant = PROPVARIANT::from(APP_ID);

		// Write it to the shortcut's property store
		if let Err(err) = property_store.SetValue(&PKEY_AppUserModel_ID, &prop_variant) {
	        error!("[Notifications] TODO: {:?}", err);
	        return;
	    }
        
        // Write the App ID shortcut's metadata
        if let Err(err) = property_store.SetValue(&PKEY_AppUserModel_ID, &prop_variant) {
	        error!("[Notifications] Failed set App ID: {:?}", err);
	        return;
	    }
        if let Err(err) = property_store.Commit() {
	        error!("[Notifications] Failed commit metadata: {:?}", err);
	        return;
	    }

        // Cast to IPersistFile to save the link to the disk
        let persist_file: IPersistFile = match shell_link.cast() {
    	Ok(file) => file,
	        Err(err) => {
	        	error!("[Notifications] Failed to cast IShellLinkW to IPersistFile: {:?}", err);
	        	return;
	        }
	    };
        
        // Save shortcut file to disk
        if let Err(err) = persist_file.Save(PCWSTR::from_raw(save_path_wide.as_ptr()), true) {
	        error!("[Notifications] Failed to save shortcut to disk: {:?}", err);
	        return;
	    }
    }
    debug!("[Notifications] Shortcut added to Start Menu\\Programs");

}


/// Convert standard Rust strs into null-terminated UTF-16 wide strings for windows
fn to_widestr(value: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = value.encode_utf16().collect();
    wide.push(0); // Add the null terminator
    wide
}