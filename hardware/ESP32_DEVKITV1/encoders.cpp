#include <Arduino.h>
#include <ESP32Encoder.h>

// Encoder 1: Volume Controller Pins
#define VOL_A 18
#define VOL_B 19

// Encoder 2: Navigation Pins
#define NAV_A 23
#define NAV_B 22

// Mute button pin
#define MUTE_PIN 13

// Encoder objects
ESP32Encoder volEncoder;
ESP32Encoder navEncoder;

// State variables
int64_t lastVolCount = 0;
int64_t lastNavCount = 0;
int lastButtonState = HIGH;

// Time variable to filter out bounces (bounces above 12.8 microseconds, a problem for cheap encoders)
unsigned long lastVolTime = 0;
unsigned long lastNavTime = 0;
unsigned long lastMuteTime = 0;
const unsigned long DEBOUNCE_DELAY = 30;


void initEncoders() {
  // Pullup mute button
  pinMode(MUTE_PIN, INPUT_PULLUP);

  // Pullup encoders
  ESP32Encoder::useInternalWeakPullResistors = puType::up;

  // Setup
  volEncoder.attachSingleEdge(VOL_A, VOL_B);
  navEncoder.attachSingleEdge(NAV_A, NAV_B);
  // Debounce (filters bounces under 12.8 microseconds, prevents incorrect values)
  volEncoder.setFilter(1023);
  navEncoder.setFilter(1023);

  // Establish initial positions
  volEncoder.clearCount();
  navEncoder.clearCount();
  lastVolCount = volEncoder.getCount();
  lastNavCount = navEncoder.getCount();

}

// Get change in volume encoder position
int volDelta() {
  int64_t currentVolCount = volEncoder.getCount();
  unsigned long now = millis();
  int delta = 0;

  if (now - lastVolTime >= DEBOUNCE_DELAY) {
    // Non-zero delta only if delay has passed
    delta = (int) (currentVolCount - lastVolCount);
    if (delta != 0) { lastVolTime = now; }
  }
  lastVolCount = currentVolCount;
  return delta;
}
// Get change in navigation encoder position
int navDelta() {
  int64_t currentNavCount = navEncoder.getCount();
  unsigned long now = millis();
  int delta = 0;
  
  if (now - lastNavTime >= DEBOUNCE_DELAY) {
    // Non-zero delta only if delay has passed
    delta = (int) (currentNavCount - lastNavCount);
    if (delta != 0) { lastNavTime = now; }
  }
  lastNavCount = currentNavCount;
  return delta;
}

// Returns true exacly once each time the mute button is pushed
bool muteCheck() {
  int buttonState = digitalRead(MUTE_PIN);
  unsigned long now = millis();

  if (now - lastMuteTime >= DEBOUNCE_DELAY) {
    if (buttonState != lastButtonState) {
      lastButtonState = buttonState;
      if (buttonState == LOW){
        //Button pressed
        return true;
      }
    }
  }
  return false;
}




