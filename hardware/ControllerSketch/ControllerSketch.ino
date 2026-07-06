#include <Arduino.h>

#define BUILTIN_LED 2

void setup() {
  // Light setup
  pinMode(BUILTIN_LED, OUTPUT);

  // Init serial port
  Serial.begin(921600);
  // Wait for serial port to connect
  while (!Serial) { ; }
}

void loop() {
  // check if new serial data
  if (Serial.available() > 0) {
    serial_input();
  }

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
