
#include <Arduino.h>
#include "DIY_TFT_Display.h"

#define LINE_GAP 8 // pixels between lines of text (top to top)

// Initialize display with custom ESP32 pin configuration; TODO: Get this from config
// Pins used: D0=17, D1=16, D2=26, D3=25, D4=21, D5=5, D6=27, D7=14, RD=2, WR=4, CD/DC=15, CS=33, RST=32
Display::Display() : lcd (17, 16, 26, 25, 21, 5, 27, 14, 2, 4, 15, 33, 32) {
  // init settings to defaults
  rotation = 1;
  text_size = 3;
  text_color = 0xFFFF;
  bk_color = 0x0000;
  text_len = 20;
  side_pad = 30;

  // Load settings from persistant storage
  stored.begin("Display", true); // open storage in read only
  if (stored.isKey("rotation")) {
    rotation = stored.getUChar("rotation");
  }
  if (stored.isKey("text_color")) {
    text_color = stored.getUShort("text_color");
  }
  if (stored.isKey("bk_color")) {
    bk_color = stored.getUShort("bk_color");
  }
  stored.end();


  // Calculate padding
  mid_pad = (lcd.height() / 4) - (8 * text_size);
  top_pad = mid_pad / 2;

  lcd.begin(); // Init screen; MUST ONLY BE CALLED ONCE
  setup();
  show_disconnected();
}

void Display::setup() {
  lcd.setRotation(rotation);
  lcd.fillScreen(bk_color);
  lcd.setTextColor(text_color, bk_color);
  lcd.setTextSize(text_size);
  // Draw selection box
  int box_top = ((top_pad * 2) + mid_pad + (text_size * 16)) / 2;
  int box_bot = ((top_pad * 2) + (mid_pad * 3) + (text_size * 16)) / 2;
  lcd.drawRoundRect(
    side_pad / 2, 
    box_top, 
    lcd.width() - side_pad, 
    box_bot - box_top, 
    10, 
    text_color);
}

// Render a frame to the display
// Uses LineState structs to only overwrite necessary pixels
void Display::render_frame(DisplayFrame frame) {
  int text_height = 8 * text_size;
  int text_width = 6 * text_size;

  // Clear disconnected message if needed
  clear_disconnected();

  char buffer[text_len + 1] = {};
  for (int i = 0; i < 3; i++) {
    int y_pos = top_pad + (i * mid_pad);
    // Name
    snprintf(buffer, sizeof(buffer), "%s", frame.slots[i].name);
    lcd.setCursor(side_pad, y_pos);
    lcd.print(buffer);
    // Clear rest of name space if needed
    int cursor_x = lcd.getCursorX();
    if (cursor_x < frame_state[i].name_end) {
      lcd.fillRect(
        cursor_x, 
        y_pos, 
        frame_state[i].name_end - cursor_x, 
        text_height, 
        bk_color
      );
    }
    frame_state[i].name_end = cursor_x; // set frame state

    // Info
    
    // If mute state changed
    if (frame_state[i].muted != frame.slots[i].muted) {
      if (frame.slots[i].muted) {
        lcd.setCursor(side_pad + 8, y_pos + text_height + 6);
        lcd.setTextSize(text_size - 1);
        lcd.print("[MUTE]");
        lcd.setTextSize(text_size);
      } else {
        lcd.fillRect(
          side_pad + 8, 
          y_pos + text_height + 6, 
          7 * (text_width - 6), 
          text_height - 8, 
          bk_color
        );
      }
      frame_state[i].muted = frame.slots[i].muted;
    }
    
    int volume_state = (frame.slots[i].name[0] == '\0') ? -1 : frame.slots[i].volume; // negate state means do not draw
    // if volume changed
    if (frame_state[i].volume != volume_state) {
      if (volume_state < 0) {
        // Clear old volume
        lcd.fillRect(
          lcd.width() - side_pad - (4 * text_width), 
          y_pos + text_height, 
          5 * (text_width), 
          text_height, 
          bk_color
        );
      } else {
        // Print new volume
        lcd.setCursor(lcd.width() - side_pad - (4 * text_width), y_pos + text_height);
        lcd.printf("%3i%%", frame.slots[i].volume);
      }
      frame_state[i].volume = volume_state;
    }
    
  }
}

// Write to the display to show that the device is not connected
void Display::show_disconnected() {
  constexpr char* text = "NOT CONNECTED"; // 13 chars
  lcd.setCursor(
    (lcd.width() / 2) - (39 * text_size),
    (lcd.height() / 2) - (4 * text_size)
  );
  lcd.print(text);
  dc_shown = true;
}

// Clear disconnected message
void Display::clear_disconnected() {
  if (!dc_shown) {return;} // Sort circut if already clear
  lcd.fillRect(
    (lcd.width() / 2) - (39 * text_size),
    (lcd.height() / 2) - (4 * text_size),
    13 * 6 * text_size,
    9 * text_size,
    bk_color
  );
  dc_shown = false;
}

// Apply settings from DisplayConfig
void Display::apply_settings(DisplayConfig config) {
  uint8_t new_rotation = 1 + ((uint8_t) config.invert * 2);
  // Did anything change?
  if (new_rotation != rotation || config.text_color != text_color || config.bk_color != bk_color) {
    // Something changed
    rotation = new_rotation;
    text_color = config.text_color;
    bk_color = config.bk_color;

    // Save settings to persistant storage
    stored.begin("Display", false); // open storage in read/write
    stored.putUChar("rotation", rotation);
    stored.putUShort("text_color", text_color);
    stored.putUShort("bk_color", bk_color);
    stored.end();

    // Redo setup
    setup();
  }
}
