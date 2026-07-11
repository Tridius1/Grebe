use std::thread;
use crossbeam_channel::{Sender, Receiver, select};
use serialport;
use serialport::SerialPort;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::time::{Duration, Instant};
use log::{info, debug, error};

use crate::config;

// Header byte when sending frames
pub const FRAME_HEADER: u8 = 0xBB;
const ACK_BYTE: u8 = 0x06;
const COMMAND_HEADER: u8 = 0xAA;

// Command bytes
const VOLUP_CMD: u8 = 0x02;
const VOLDOWN_CMD: u8 = 0x03;
const NAVUP_CMD: u8 = 0x04;
const NAVDOWN_CMD: u8 = 0x05;
const MUTE_CMD: u8 = 0x10;
const FRAME_CMD: u8 = 0xF0;

// newtype wrapper for frame bytes
#[derive(Debug)]
pub struct FramePacket(pub [u8; crate::FRAME_SIZE]);
// allows easy writing of bytes to serial port
impl AsRef<[u8]> for FramePacket {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

// External messages (sent to coordinator)
#[derive(Debug)]
pub enum ControlMsg {
    AppScroll { up: bool }, // up is true, down is false
    VolumeScroll { up: bool }, // up is true, down is false
    MuteToggle,
    NewFrame,
}

// Internal messeges (sent between serial subsystem and reader/writer threads)
#[derive(Debug, Clone, Copy)]
enum SerialRecieved {
	Acknowledge,
	VolUp,
	VolDown,
	NavUp,
	NavDown,
	MuteToggle,
	RequestFrame,
	Error(u8)
}


pub fn run_serial_subsystem(to_coordinator: Sender<ControlMsg>, from_coordinator: Receiver<FramePacket>) {

	let port_result = serialport::new("COM3", 115200).timeout(Duration::from_millis(1)).open();
	let port = match port_result {
		Ok(p) => p,
		Err(e) => {
			eprintln!("[Serial Subsystem] Failed to open COM3. Is the microcontroller plugged in? Error: {}", e);
			return;
		}
	};

	// Create interal channels
	let (writer_packets_tx, writer_packets_rx) = crossbeam_channel::unbounded::<FramePacket>();
	let (writer_ack_tx, writer_ack_rx) = crossbeam_channel::unbounded::<SerialRecieved>();
	let (reader_tx, reader_rx) = crossbeam_channel::unbounded::<SerialRecieved>();

	// spawn reader and writer
	let port_clone = match port.try_clone() {
		Ok(cp) => cp,
		Err(e) => { eprintln!("[Serial Subsystem] Failed to clone serial port for reader."); return }
	};
	thread::spawn(move || { serial_reader(port_clone, reader_tx) } );
	thread::spawn(move || { serial_writer(port, writer_packets_rx, writer_ack_rx); } );
	
	// listen for messages
	loop {
		select!{
			recv(from_coordinator) -> msg => {
				match msg {
					Ok(packet) => {
						writer_packets_tx.send(packet);
					}
					Err(e) => { eprintln!("[Serial Subsystem] Error reading message from Coordinator.") }
				}
			}
			recv(reader_rx) -> msg => {
				match msg {
					Ok(received) => {
						match received {
							SerialRecieved::Acknowledge => { writer_ack_tx.send(received); }
							SerialRecieved::Error(b) => { info!("[Serial Subsystem] Uknown command byte:{:?}", b); }
							SerialRecieved::VolUp => { to_coordinator.send(ControlMsg::VolumeScroll { up: true }); }
							SerialRecieved::VolDown => { to_coordinator.send(ControlMsg::VolumeScroll { up: false }); }
							SerialRecieved::NavUp => { to_coordinator.send(ControlMsg::AppScroll { up: true }); }
							SerialRecieved::NavDown => { to_coordinator.send(ControlMsg::AppScroll { up: false }); }
							SerialRecieved::MuteToggle => { to_coordinator.send(ControlMsg::MuteToggle); }
							SerialRecieved::RequestFrame => { to_coordinator.send(ControlMsg::NewFrame); }
						}
					}
					Err(_) => { eprintln!("[Serial Subsystem] Error reading message from Serial Reader.") }
				}
			}
		}
	}

	
}


fn serial_reader(mut port: Box<dyn SerialPort>, reader_tx: Sender<SerialRecieved>) {
	// Single byte buffer to read headers
	let mut single_byte_buf = [0u8; 1];

	loop {
		match port.read(&mut single_byte_buf) {
			Ok(0) => {
				eprintln!("[Serial Reader] Device disconnected (EOF).");
			},
			Ok(1) => {
				debug!("Received byte: {:?}", single_byte_buf[0]);
				// match byte to headers
				match single_byte_buf[0] {
					ACK_BYTE => {
						let _ = reader_tx.send(SerialRecieved::Acknowledge);
					}
					COMMAND_HEADER => {
						loop {
							let mut payload = [0u8, 1];
							match port.read(&mut payload) {
								Ok(0) => eprintln!("[Serial Reader] Device disconnected (EOF)."),
								Ok(1) => {
									reader_tx.send(read_command(payload[0]));
									break;
								}
								Ok(_) => unreachable!(),
								Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
									std::thread::yield_now(); // If we timed out, yield and try again later
								}
								Err(e) => {
									eprintln!("[Serial Reader] Fatal read error: {}", e);
								}
							}
						}
					},
					_ => { debug!("[Serial Reader] Recived uknown header byte:  {:?}", single_byte_buf[0]); }
				}
			},
			Ok(_) => unreachable!(),
			Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
				std::thread::yield_now(); // If we timed out, yield and try again later
			}
			Err(e) => {
				eprintln!("[Serial Reader] Fatal read error: {}", e);
			}
		}
	}
	debug!("[Serial Reader] Serial reader has terminated.");
}


fn serial_writer(mut port: Box<dyn SerialPort>, packets_rx: Receiver<FramePacket>, ack_rx: Receiver<SerialRecieved>) {
	for packet in packets_rx {
		// retry 5 times
		for attempt in 1..=5 {
			// do the writing
			if port.write_all(packet.as_ref()).is_err() {
    			debug!("[Serial Writer] Could not write to serial port.");
			}
			if let Err(err) = port.flush() {
				debug!("[Serial Writer] Failed to flush serial port buffers: {}", err);
			}
			// Wait for Ack
			match ack_rx.recv_timeout(Duration::from_millis(30)) {
				Ok(_) => { debug!("[Serial Writer] Packet delivered successfully on attempt {}.", attempt); break; }
				Err(_) => { debug!("[Serial Writer] Timeout waiting for ACK. Retrying (Attempt {}/5).", attempt); }
			}
		}
		
	}
	eprintln!("[Serial Subsystem] Serial writer terminated.");
}


fn read_command(cmd_byte: u8) -> SerialRecieved {
	match cmd_byte {
		VOLUP_CMD => {
			return SerialRecieved::VolUp
		}
		VOLDOWN_CMD => {
			return SerialRecieved::VolDown
		}
		NAVUP_CMD => {
			return SerialRecieved::NavUp
		}
		NAVDOWN_CMD => {
			return SerialRecieved::NavDown
		}
		MUTE_CMD => {
			return SerialRecieved::MuteToggle
		}
	    b => {
	    	return SerialRecieved::Error(b)
	    }
	}
}
