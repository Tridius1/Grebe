use std::thread;
use crossbeam_channel::{Sender, Receiver};
use serialport;
use serialport::SerialPort;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::time::{Duration, Instant};

// External messages (sent to coordinator)
#[derive(Debug)]
pub enum ControlMsg {
    AppScroll { pid: u32, name: String }, 
    VolumeScroll { pid: u32, volume: f32, muted: bool }
}

pub fn run_serial_subsystem(to_coordinator: Sender<ControlMsg>, from_coordinator: Receiver<ControlMsg>) {

	let port_result = serialport::new("COM3", 115200).timeout(Duration::from_millis(1)).open();
	let port = match port_result {
		Ok(p) => p,
		Err(e) => {
			eprintln!("[Serial Subsystem] Failed to open COM3. Is the microcontroller plugged in? Error: {}", e);
			return;
		}
	};

	
	// spawn reader; become writer
	let port_clone = match port.try_clone() {
		Ok(cp) => cp,
		Err(e) => { eprintln!("[Serial Subsystem] Failed to clone serial port for reader."); return }
	};
	thread::spawn(move || { serial_reader(port_clone, to_coordinator) } );

	serial_writer(port, from_coordinator);

	
}


fn serial_reader(port: Box<dyn SerialPort>, to_coordinator: Sender<ControlMsg>) {

	
}


fn serial_writer(port: Box<dyn SerialPort>, from_coordinator: Receiver<ControlMsg>) {

	
}
