#define BUILTIN_LED 2

// Declare interrupt functions
void IRAM_ATTR VolumeEncInterrupt(); 

// Volume encoder pins
const int VOLENC_A = 32;
const int VOLENC_B = 33;


// Volume encoder
volatile int volumeEncChange = 0;
volatile int volumeEncState = LOW;
volatile unsigned long lastInterruptTime = 0;
const unsigned long DEBOUNCE_DELAY = 3;

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

  // Configure Encoder 1
  pinMode(VOLENC_A, INPUT_PULLUP);
  pinMode(VOLENC_B, INPUT_PULLUP);
  attachInterrupt(digitalPinToInterrupt(VOLENC_A), VolumeEncInterrupt, CHANGE);
  volumeEncState = digitalRead(VOLENC_A);


  while (!Serial) { ; }
  delay(500);
  Serial.println("Ready");
}

void loop() {

  // Volume encoder handling
  if (volumeEncChange != 0) {
    noInterrupts();
    int change = volumeEncChange;
    volumeEncChange = 0;
    interrupts();
    Serial.print("Change: ");
    Serial.println(change);
    if (change == 1) {
      Serial.println("Vol +");
    }
    if (change == -1) {
      Serial.println("Vol -");
    }
    change = 0;
  }

  // check if new serial data
  //if (Serial.available() > 0) {
  //  serial_input();
  //}

}


void IRAM_ATTR VolumeEncInterrupt() {
  unsigned long currentTime = millis();

  if (currentTime - lastInterruptTime > DEBOUNCE_DELAY) {
    int newVolumeEncState = digitalRead(VOLENC_A);
    int volEncB = digitalRead(VOLENC_B);

    if (newVolumeEncState != volumeEncState) {
      if (volEncB != newVolumeEncState) {
        // rotating clockwise (+)
      volumeEncChange = 1;
      } else {
        // rotating counter-clockwise (-)
        volumeEncChange = -1;
      }
    }
    volumeEncState = newVolumeEncState;
    lastInterruptTime = currentTime;
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
