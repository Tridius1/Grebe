
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

// Initialize display with custom ESP32 pin configuration; TODO: Get this from config
// Pins used: D0=12, D1=13, D2=26, D3=25, D4=21, D5=5, D6=27, D7=14, RD=2, WR=4, CD/DC=15, CS=33, RST=32
DIYables_TFT_ILI9486_Shield tft(17, 16, 26, 25, 21, 5, 27, 14, 2, 4, 15, 33, 32);

void initScreen() {
  //Serial.println("initScreen");
  tft.begin();
  tft.setRotation(3); // Set to Landscape Mode
  
  // Clear the screen with a clean Blue color background
  tft.fillScreen(0x001F);
  
  // Draw an overlapping graphic rectangle to visually verify pixel mapping
  tft.fillRect(40, 40, 200, 100, COLOR_CYAN);
  
  // Set up text parameters using the active Adafruit GFX base features
  tft.setTextColor(COLOR_BLACK);
  tft.setTextSize(2);
  tft.setCursor(55, 65);
  //tft.print("Screen Init");
  
  tft.setCursor(55, 95);
  tft.print("ESP32 Status: OK!");
  //Serial.println("LCD Initilized");
}