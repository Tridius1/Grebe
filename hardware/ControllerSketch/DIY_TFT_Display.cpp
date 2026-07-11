
#include <Arduino.h>
#include "DIY_TFT_Display.h"
#include <DIYables_TFT_Shield.h> // DIYables Screen Library

// Standard 16-bit RGB565 color definitions
#define COLOR_BLACK   0x0000
#define COLOR_BLUE    0x001F
#define COLOR_RED     0xF800
#define COLOR_GREEN   0x07E0
#define COLOR_CYAN    0x07FF
#define COLOR_MAGENTA 0xF81F
#define COLOR_YELLOW  0xFFE0
#define COLOR_WHITE   0xFFFF

#define LINE_GAP 8 // pixels between lines of text (top to top)


Display::Display(uint8_t rot) : lcd (17, 16, 26, 25, 21, 5, 27, 14, 2, 4, 15, 33, 32) {
  // init settings
  rotation = rot;
  text_size = 3;
  text_color = COLOR_BLACK;
  bk_color = COLOR_CYAN;
  text_len = 24;

  lcd.begin(); // Init screen; MUST ONLY BE CALLED ONCE

  setup();
}

void Display::setup() {
  lcd.setRotation(rotation);
  lcd.fillScreen(bk_color);
  lcd.setTextColor(text_color, bk_color);
  lcd.setTextSize(text_size);
}

void Display::render_frame(DisplayFrame frame) {
  char buffer[text_len];

  for (int i = 0; i < 3; i++) {
    // Name
    snprintf(buffer, sizeof(buffer), "%s", frame.slots[i].name);
    lcd.setCursor(left_pad, top_pad + (i * mid_pad));
    lcd.print(buffer);
    // Info
    String vol_string = String(frame.slots[i].volume);
    String mute_string = (frame.slots[i].muted) ? "[MUTED]" : "";
    snprintf(buffer, sizeof(buffer), "%-*s%s%%", vol_string.length(), mute_string.c_str(), vol_string.c_str());
    lcd.setCursor(left_pad, top_pad + (i * mid_pad) + (8 * text_size));
    lcd.print(buffer);
  }  
}


/*
// Initialize display with custom ESP32 pin configuration; TODO: Get this from config
// Pins used: D0=12, D1=13, D2=26, D3=25, D4=21, D5=5, D6=27, D7=14, RD=2, WR=4, CD/DC=15, CS=33, RST=32
DIYables_TFT_ILI9486_Shield tft(17, 16, 26, 25, 21, 5, 27, 14, 2, 4, 15, 33, 32);

void initScreen() {
  //Serial.println("initScreen");
  tft.begin();
  tft.setRotation(3); // Set to Landscape Mode
  
  // Clear the screen with a clean Blue color background
  tft.fillScreen(COLOR_CYAN);
  
  // Set up text parameters using the active Adafruit GFX base features
  tft.setTextColor(COLOR_BLACK);
  tft.setTextSize(3);
  tft.setCursor(0, 65);
  tft.print("012345678901234567890123456789");
  
  tft.setCursor(55, 95);
  tft.print("ESP32 Status: OK!");
}
*/