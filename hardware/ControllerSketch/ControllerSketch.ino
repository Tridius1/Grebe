#include "encoders.h"
#include "DIY_TFT_Display.h"

#define BUILTIN_LED 2

#define CMD_HEADER 0xAA
#define FRAME_HEADER 0xBB
#define CONFIG_HEADER 0xCC
#define ACK_BYTE 0x06

#define VOLUP_CMD 0x02
#define VOLDOWN_CMD 0x03
#define NAVUP_CMD 0x04
#define NAVDOWN_CMD 0x05
#define MUTE_CMD 0x10
#define CONFIG_REQ 0x60

#define HEARTBEAT 0xEE

// Wait before resending requests
unsigned long lastReqTime = 0;
const unsigned long REQ_COOLDOWN = 100;

// Heartbeats expected every 5 seconds
unsigned long lastHeartbeatTime = 0;

// Screen saver time check
unsigned long lastRefreshTime;
const unsigned long REFRESH_COOLDOWN = 1200000; // milliseconds between screen-saver refreshes (20 minutes)

void send_cmd_byte(uint8_t cmd) {
  uint8_t packet[2] = {CMD_HEADER, cmd};
  Serial.write(packet, 2);
}

void send_ack() {
  Serial.write(ACK_BYTE);
}

Display* LCD = nullptr;

bool loaded_config; // Have we recived a config from PC?

void setup() {
  // Light setup
  pinMode(BUILTIN_LED, OUTPUT);

  // Init serial port
  Serial.begin(921600);
  // Wait for serial port to connect
  while (!Serial) { ; }
  delay(500);
  //Serial.println("Ready");

  // Setup screen
  //initScreen();
  LCD = new Display();

  initEncoders();

  loaded_config = false;

  lastHeartbeatTime = millis();
  lastRefreshTime = millis();
}

void loop() {
  unsigned long now = millis();
  // Check if heartbeats are missing
  if (now - lastHeartbeatTime >= 15000) {
    // 15 seconds = 5 missed heartbeats
    LCD -> show_disconnected();
  }

  // Config
  if (!loaded_config) {
    if (now - lastReqTime >= REQ_COOLDOWN) {
      send_cmd_byte(CONFIG_REQ);
      lastReqTime = now;
    }
  }

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

  // check if new serial data
  if (Serial.available() > 0) {
    serial_input();
  }

  // Screen-saver check
  if (now - lastRefreshTime >= REFRESH_COOLDOWN) {
    LCD -> refresh_sweep();
    lastRefreshTime = millis();
  }

  delay(1);
}



// Called whenever a character arrives from the serial port
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
      break;
    default:
      return;
  }
  // Any known signal counts as a heartbeat
  lastHeartbeatTime = millis();
}


