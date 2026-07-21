#include "encoders.h"
#include "DIY_TFT_Display.h"

// BYTES AGREED BY GREBE DAEMON
// Header bytes
#define CMD_HEADER 0xAA // Sent
#define FRAME_HEADER 0xBB // Received
#define CONFIG_HEADER 0xCC // Received
#define ACK_BYTE 0x06 // Sent
// Command bytes
#define VOLUP_CMD 0x02 // Sent
#define VOLDOWN_CMD 0x03 // Sent
#define NAVUP_CMD 0x04 // Sent
#define NAVDOWN_CMD 0x05 // Sent
#define MUTE_CMD 0x10 // Sent
#define CONFIG_REQ 0x60 // Sent
// Heartbeat byte
#define HEARTBEAT 0xEE // Received

// TIMING GLOBALS
// Wait before resending requests
unsigned long lastReqTime = 0;
const unsigned long REQ_COOLDOWN = 100;
// Heartbeats expected every 5 seconds
unsigned long lastHeartbeatTime = 0;
const unsigned long HEARTBEAT_TIMEOUT = 15000; // 15 seconds = 5 heartbeats
// Screen saver time check
unsigned long lastRefreshTime;
const unsigned long REFRESH_COOLDOWN = 1200000; // milliseconds between screen-saver refreshes (20 minutes)

// OTHER GLOBALS
Display* LCD = nullptr;
bool loaded_config = false; // Have we recived a config from PC?

// HELPER FUNCTIONS
// Attach expected header and send command byte
void send_cmd_byte(uint8_t cmd) {
  uint8_t packet[2] = {CMD_HEADER, cmd};
  Serial.write(packet, 2);
}
// Send an acknoledgement
// Single byte response expected after frames or config data
void send_ack() {
  Serial.write(ACK_BYTE);
}

// CORE FUNCTIONALITY
void setup() {
  // Light setup
  pinMode(BUILTIN_LED, OUTPUT);

  // Init serial port
  Serial.begin(921600);
  // Wait for serial port to connect
  while (!Serial) { ; }
  delay(500);

  // Setup screen
  LCD = new Display();

  // Setup encoders
  initEncoders();

  // Init timers
  lastHeartbeatTime = millis();
  lastRefreshTime = millis();
}

void loop() {
  // Check for new serial data
  if (Serial.available() > 0) {
    serial_input();
  }

  // HANDLE INPUTS
  // Volume encoder handling
  int volumeChange = volDelta();
  if (volumeChange != 0) {
    if (volumeChange > 0) {
      send_cmd_byte(VOLUP_CMD);
    } else {
      send_cmd_byte(VOLDOWN_CMD);
    }
  }
  // Navigation encoder handling
  int navigationChange = navDelta();
  if (navigationChange != 0) {
    if (navigationChange > 0) {
      send_cmd_byte(NAVUP_CMD);
    } else {
      send_cmd_byte(NAVDOWN_CMD);
    }
  }
  // Check for mute button press
  if (muteCheck()) {
    send_cmd_byte(MUTE_CMD);
  }

  // TIME BASED EVENTS
  unsigned long now = millis();
  // Request config if not yet loaded
  if (!loaded_config) {
    if (now - lastReqTime >= REQ_COOLDOWN) {
      send_cmd_byte(CONFIG_REQ);
      lastReqTime = now;
    }
  }
  // Check if heartbeats are missing
  if (now - lastHeartbeatTime >= HEARTBEAT_TIMEOUT) {
    LCD -> show_disconnected();
  }
  // Screen-saver check
  if (now - lastRefreshTime >= REFRESH_COOLDOWN) {
    LCD -> refresh_sweep();
    lastRefreshTime = millis();
  }
}

// Called whenever data arrives at the serial port
void serial_input() {
  char incomingByte = Serial.read();

  switch (incomingByte) {
    case FRAME_HEADER:
      // New frame
      DisplayFrame ui_frame;
      Serial.readBytes((char*)&ui_frame, sizeof(DisplayFrame));
      // Send ACK byte
      send_ack();
      // Set the new frame
      LCD -> set_frame(ui_frame);
      // Write new frame if config loaded
      if (loaded_config) {LCD -> render_frame();}
      break;
    case CONFIG_HEADER:
      // Read config
      DisplayConfig new_config;
      Serial.readBytes((char*)&new_config, sizeof(DisplayConfig));
      // Send ACK byte
      send_ack();
      // Set config then re-render frame
      LCD -> apply_settings(new_config);
      LCD -> render_frame();
      // Mark config loaded
      loaded_config = true;
      break;
    case HEARTBEAT:
      // If in a disconnected state, update to show connection
      if (LCD -> get_dc() == false) {
        LCD -> render_frame();
      }
      break;
    default:
      // If incomming byte not recognized return before setting lastHeartbeatTime
      return;
  }
  // Any known signal counts as a heartbeat
  lastHeartbeatTime = millis();
}


