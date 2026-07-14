use std::thread;
use crossbeam_channel::{Sender, Receiver, select};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use serialport;
use serialport::SerialPort;
use std::time::{Duration, Instant};
use log::{info, debug, error};

use crate::notify;
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

// newtype wrapper for frame bytes
#[derive(Clone, Debug)]
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
	Error(u8)
}

fn serial_subsystem(port: Box<dyn SerialPort>, to_coordinator: Sender<ControlMsg>, from_coordinator: Receiver<FramePacket>){
	let running_flag = Arc::new(AtomicBool::new(true)); // Flag for closing child threads

	// Create interal channels
	let (writer_packets_tx, writer_packets_rx) = crossbeam_channel::unbounded::<FramePacket>();
	let (writer_ack_tx, writer_ack_rx) = crossbeam_channel::unbounded::<SerialRecieved>();
	let (reader_tx, reader_rx) = crossbeam_channel::unbounded::<SerialRecieved>();

	// spawn reader and writer
	let port_clone = match port.try_clone() {
		Ok(cp) => cp,
		Err(e) => { error!("[Serial Subsystem] Failed to clone serial port for reader: {}", e); return }
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
						let _ = writer_packets_tx.send(packet);
					}
					Err(e) => { error!("[Serial Subsystem] Error reading message from Coordinator: {}", e) }
				}
			}
			recv(reader_rx) -> msg => {
				match msg {
					Ok(received) => {
						match received {
							SerialRecieved::Acknowledge => { let _ = writer_ack_tx.send(received); }
							SerialRecieved::Error(b) => { info!("[Serial Subsystem] Uknown command byte:{:?}", b); }
							SerialRecieved::VolUp => { let _ = to_coordinator.send(ControlMsg::VolumeScroll { up: true }); }
							SerialRecieved::VolDown => { let _ = to_coordinator.send(ControlMsg::VolumeScroll { up: false }); }
							SerialRecieved::NavUp => { let _ = to_coordinator.send(ControlMsg::AppScroll { up: true }); }
							SerialRecieved::NavDown => { let _ = to_coordinator.send(ControlMsg::AppScroll { up: false }); }
							SerialRecieved::MuteToggle => { let _ = to_coordinator.send(ControlMsg::MuteToggle); }
						}
					}
					Err(e) => { error!("[Serial Subsystem] Error reading message from Serial Reader: {}", e); break; }
				}
			}
		}
	}
	// Ensure child threads are closed
	running_flag.store(false, Ordering::Relaxed);
	let _ = reader_handle.join();
	let _ = writer_handle.join();
	info!("[Serial Subsystem] Microcontroller is disconnected. Will attempt to reconnect.");

}

// wrapper function to retry connecion
pub fn run_serial_subsystem(to_coordinator: Sender<ControlMsg>, from_coordinator: Receiver<FramePacket>) {
	let port_name = config::get().port.clone();
	let mut retry_timeout = Duration::from_millis(500);
	let mut first_connect = true;

	loop {
		let port_result = serialport::new(&port_name, 921600).timeout(Duration::from_millis(10)).open();
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
				if retry_timeout < Duration::from_secs(7) {
					retry_timeout = retry_timeout * 2;
				}
				continue;
			}
		};

		info!("[Serial Subsystem] Connected to microcontroller on {}.", port_name);

		let notif_cfg = &config::get().notifications;

		// Send a notification if enabled
		if (notif_cfg.on_first_connect && first_connect) || (notif_cfg.on_reconnect && !first_connect) {
			let body = format!("Communicating with mixer on {}", port_name);
			if let Err(e) = notify::send_notification("Mixer Connected", &body) {
	        	error!("Failed to send notification: {:?}", e);
	    	}
	    }
	    first_connect = false;

	    // Start the subsystem
		serial_subsystem(port, to_coordinator.clone(), from_coordinator.clone());

		// Disconnected; send a notification if enabled
		if notif_cfg.on_disconnect {
			let body = format!("Waiting for mixer to reconnect on {}", port_name);
			if let Err(e) = notify::send_notification("Mixer Disconnected", &body) {
	        	error!("Failed to send notification: {:?}", e);
	    	}
		}

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
									let _ = reader_tx.send(read_command(payload[0]));
									break;
								}
								Ok(_) => unreachable!(),
								Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
									std::thread::yield_now(); // If we timed out, yield and try again later
								}
								Err(e) => {
									error!("[Serial Reader] Fatal read error: {}", e);
								}
							}
						}
					},
					_ => { debug!("[Serial Reader] Received unknown header byte:  {:?}", single_byte_buf[0]); }
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
	let mut packet: Option<FramePacket> = None;
	let mut attempt: u8 = 0;
	let mut unsent_waiting: bool = false; // flag to ensure most recent packet is sent
	let mut timer = Instant::now(); // timer to ensure packets are not sent too fast
	while running_flag.load(Ordering::Relaxed) {  
		if packet.is_some() {
			// We have a packet, send it if it's not too soon
			if timer.elapsed() >= Duration::from_millis(10) {
				// Call OS to send packet over USB
				if port.write_all(packet.clone().expect("[Serial Writer] Packet.expect failed while is_some()").as_ref()).is_err() {
	    			debug!("[Serial Writer] Could not write to serial port.");
	    			break;
				}
				// Force OS to send any remaining data and block until data has been sent
				if let Err(err) = port.flush() {
					debug!("[Serial Writer] Failed to flush serial port buffers: {}", err);
					break;
				}
				attempt += 1;
				timer = Instant::now(); // reset the timer
				debug!("[Serial Writer] Packet written to serial port.");
			}
			// Just sent a packet; wait for ack and replace old packets (only send new packets)
			select!{
				// replacing old packets if they arrive before acknoledgement
				recv(packets_rx) -> new_packet => {
					match new_packet {
			        	Ok(p) => {
							debug!("[Serial Writer] Replaced old packet before acknowledgment.");
							packet = Some(p);
							unsent_waiting = true; // Make sure this new packet is sent
			        	}
			        	Err(e) => {
			        		debug!("[Serial Writer] Error receiving packets from Serial Subsystem: {}", e);
			        	}
			    	}
				}
				// recived ack, we're done
				recv(ack_rx) -> _ => {
					debug!("[Serial Writer] Packet delivered successfully on attempt {}.", attempt);
					attempt = 0;
					if unsent_waiting {
						// If a packet has replaced the last one and sit unsent it should be sent on the next loop, not cleared
						unsent_waiting = false;
					} else {
						packet = None;
					}
				}
				// timeout
				default(Duration::from_millis(50)) => {
					debug!("[Serial Writer] Sent packet was not acknowleded after 50ms.");
				}
			}
			// abort after 5 attempts
			if packet.is_some() && attempt > 5 {
				debug!("[Serial Writer] Failed to deliver packet after 5 attempts.");
				packet = None;
			}
		} else {
			// No packet, wait for one from serial subsystem
			match packets_rx.recv_timeout(Duration::from_millis(50)) {
	        	Ok(p) => packet = Some(p),
		 		Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue, // Loop back and check running_flag
	        	Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break, // Channel closed, exit loop
	    	};
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
