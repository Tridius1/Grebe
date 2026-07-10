#include "encoders.h"
#include "DIY_TFT_Display.h"

#define BUILTIN_LED 2


struct __attribute__((packed)) MixerStatus {
  uint8_t volume;
  char name[31]; // Brings the total size to 32 bytes
};

void setup() {
  // Light setup
  pinMode(BUILTIN_LED, OUTPUT);

  // Init serial port
  Serial.begin(115200);
  // Wait for serial port to connect
  while (!Serial) { ; }
  delay(500);
  Serial.println("Ready");

  // Setup screen
  initScreen();

  initEncoders();
}

void loop() {

  // Volume encoder handling
  int volumeChange = volDelta();
  if (volumeChange != 0) {
    Serial.print("Volume change: ");
    Serial.println(volumeChange);
  }
  // Navigation encoder handling
  int navigationChange = navDelta();
  if (navigationChange != 0) {
    Serial.print("Navigation change: ");
    Serial.println(navigationChange);
  }


  // check if new serial data
  //if (Serial.available() > 0) {
  //  serial_input();
  //}
  delay(1);
}



// Called whenever a character arrives from the serial port
void serial_input() {

  char incomingByte = Serial.read();
  switch (incomingByte) {
    case 'A':
      // Acknowledge; send back OK
      Serial.println("OK");
      break;
    default:
      break;
  }
  if (Serial.available() > 0) {
    String incomingData = Serial.readStringUntil('\n');
    Serial.print("Unhandled Data: ");
    Serial.println(incomingData);
  }

}
