use std::thread;
use crossbeam_channel::{Sender, Receiver, select};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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

fn serial_subsystem(mut port: Box<dyn SerialPort>, to_coordinator: Sender<ControlMsg>, from_coordinator: Receiver<FramePacket>){
	let running_flag = Arc::new(AtomicBool::new(true)); // Flag for closing child threads

	// Create interal channels
	let (writer_packets_tx, writer_packets_rx) = crossbeam_channel::unbounded::<FramePacket>();
	let (writer_ack_tx, writer_ack_rx) = crossbeam_channel::unbounded::<SerialRecieved>();
	let (reader_tx, reader_rx) = crossbeam_channel::unbounded::<SerialRecieved>();

	// spawn reader and writer
	let port_clone = match port.try_clone() {
		Ok(cp) => cp,
		Err(e) => { eprintln!("[Serial Subsystem] Failed to clone serial port for reader."); return }
	};
	let rd_run_flag = Arc::clone(&running_flag);
	let wr_run_flag = Arc::clone(&running_flag);

	let reader_handle = thread::spawn(move || { serial_reader(rd_run_flag, port_clone, reader_tx) } );
	let writer_handle = thread::spawn(move || { serial_writer(wr_run_flag, port, writer_packets_rx, writer_ack_rx); } );
	
	// listen for messages
	while running_flag.load(Ordering::Relaxed) {
		select!{
			recv(from_coordinator) -> msg => {
				match msg {
					Ok(packet) => {
						writer_packets_tx.send(packet);
					}
					Err(e) => { eprintln!("[Serial Subsystem] Error reading message from Coordinator: {}", e) }
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
					Err(e) => { debug!("[Serial Subsystem] Error reading message from Serial Reader: {}", e); break; }
				}
			}
		}
	}
	// Ensure child threads are closed
	running_flag.store(false, Ordering::Relaxed);
	reader_handle.join();
	writer_handle.join();
	debug!("[Serial Subsystem] A fatal error occured, threads have been shut down. Will attempt to reconnect.");
}

// wrapper function to retry connecion
pub fn run_serial_subsystem(to_coordinator: Sender<ControlMsg>, from_coordinator: Receiver<FramePacket>) {
	let port_name = config::get().port.clone();
	let mut retry_timeout = Duration::from_millis(500);

	loop {
		let port_result = serialport::new(&port_name, 115200).timeout(Duration::from_millis(10)).open();
		let port = match port_result {
			Ok(p) => {
				retry_timeout = Duration::from_millis(500); // reset retry for next disconnect
				p
			},
			Err(e) => {
				debug!("[Serial Subsystem] Failed to open {}. Is the microcontroller plugged in? Error: {}", port_name, e);
				// Wait here for a bit before retrying
				let timer_start = Instant::now();
				while timer_start.elapsed() < retry_timeout {
					// Shutdown flag check can be placed here if needed
                    thread::sleep(Duration::from_millis(100));
                }
                // Increase timeout for exp backoff, max 10 seconds
				if retry_timeout < Duration::from_secs(10) {
					retry_timeout = retry_timeout * 2;
				}
				continue;
			}
		};

		debug!("[Serial Subsystem] Connected on {}.", port_name);
		serial_subsystem(port, to_coordinator.clone(), from_coordinator.clone());
	}
}


fn serial_reader(running_flag: Arc<AtomicBool>, mut port: Box<dyn SerialPort>, reader_tx: Sender<SerialRecieved>) {
	// Single byte buffer to read headers
	let mut single_byte_buf = [0u8; 1];

	while running_flag.load(Ordering::Relaxed) {
		match port.read(&mut single_byte_buf) {
			Ok(0) => {
				debug!("[Serial Reader] Device disconnected (EOF).");
			},
			Ok(1) => {
				debug!("Received byte: {:?}", single_byte_buf[0]);
				// match byte to headers
				match single_byte_buf[0] {
					ACK_BYTE => {
						let _ = reader_tx.send(SerialRecieved::Acknowledge);
					}
					COMMAND_HEADER => {
						while running_flag.load(Ordering::Relaxed) {
							let mut payload = [0u8, 1];
							match port.read(&mut payload) {
								Ok(0) => debug!("[Serial Reader] Device disconnected (EOF)."),
								Ok(1) => {
									reader_tx.send(read_command(payload[0]));
									break;
								}
								Ok(_) => unreachable!(),
								Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
									std::thread::yield_now(); // If we timed out, yield and try again later
								}
								Err(e) => {
									debug!("[Serial Reader] Fatal read error: {}", e);
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
				debug!("[Serial Reader] Fatal read error: {}", e);
				break;
			}
		}
	}
	debug!("[Serial Reader] Serial reader has terminated.");
	running_flag.store(false, Ordering::Relaxed); 
}


fn serial_writer(running_flag: Arc<AtomicBool>, mut port: Box<dyn SerialPort>, packets_rx: Receiver<FramePacket>, ack_rx: Receiver<SerialRecieved>) {
	while running_flag.load(Ordering::Relaxed) {
		// Get packets from serial subsystem
		let packet = match packets_rx.recv_timeout(Duration::from_millis(50)) {
        	Ok(p) => p,
        	Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue, // Loop back and check running_flag
        	Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break, // Channel closed, exit loop
    	};
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
			match ack_rx.recv_timeout(Duration::from_millis(50)) {
				Ok(_) => { debug!("[Serial Writer] Packet delivered successfully on attempt {}.", attempt); break; }
				Err(_) => { debug!("[Serial Writer] Timeout waiting for ACK. Retrying (Attempt {}/5).", attempt); }
			}
		}
		
	}
	debug!("[Serial Subsystem] Serial writer terminated.");
	running_flag.store(false, Ordering::Relaxed);
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
