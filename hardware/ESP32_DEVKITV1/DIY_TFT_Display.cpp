#include <Arduino.h>
#include "DIY_TFT_Display.h"

#define LINE_GAP 8 // pixels between lines of text (top to top)

// Initialize display with custom ESP32 pin configuration
// Pins used: D0=17, D1=16, D2=26, D3=25, D4=21, D5=5, D6=27, D7=14, RD=2, WR=4, CD/DC=15, CS=33, RST=32
Display::Display() : lcd (17, 16, 26, 25, 21, 5, 27, 14, 2, 4, 15, 33, 32) {
  // Init settings to defaults
  rotation = 1;
  text_size = 3;
  text_color = 0xFFFF;
  bk_color = 0x0000;
  text_len = 20;

  // Padding
  true_side_pad = 30; // MUST BE >3 TO PREVENT UNDERFLOW
  mid_pad = (lcd.height() / 4) - (8 * text_size);
  true_top_pad = mid_pad / 2;
  cycle_padding(); // Sets top_pad and side_pad

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

  lcd.begin(); // Init screen; MUST ONLY BE CALLED ONCE
  show_disconnected();
}

// Adjust padding to prevent burn-in
void Display::cycle_padding() {
  pad_cycle = (pad_cycle + 1) % 4;
  switch (pad_cycle) {
    case 0:
      side_pad = true_side_pad - 1;
      top_pad = true_top_pad + 3;
      break;
    case 1:
      side_pad = true_side_pad + 1;
      top_pad = true_top_pad - 3;
      break;
    case 2:
      side_pad = true_side_pad - 1;
      top_pad = true_top_pad - 3;
      break;
    case 3:
      side_pad = true_side_pad + 1;
      top_pad = true_top_pad + 3;
      break;
  }
}

// Draws the background color and selection box
// Boolean parameter determines if background is redrawn, true by default
void Display::backdrop(bool fill_bk) {
  lcd.setRotation(rotation);
  if (fill_bk) {lcd.fillScreen(bk_color);}
  lcd.setTextColor(text_color, bk_color);
  lcd.setTextSize(text_size);
  // Draw selection box based on padding
  // This means the box moves when the padding changes, preventing burn-in
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

// Public function setting the current frame to be drawn
void Display::set_frame(DisplayFrame frame) {
  current_frame = frame;
  frame_exist = true;
}

// Render the current frame to the display
// Uses LineState structs to only overwrite necessary pixels
// render_all parameter indicates that text that was drawn on the last frame should be redrawn
void Display::render_frame(bool render_all) {
  if (!frame_exist) {return;} // Abort if no frame exists
  // If a disconnect message was shown all text must be redrawn this frame
  bool force = render_all || dc_shown;

  // Clear disconnected message if needed
  clear_disconnected();

  int text_height = 8 * text_size;
  int text_width = 6 * text_size;

  for (int i = 0; i < 3; i++) {
    // Cursor y-axis position for app name
    int y_pos = top_pad + (i * mid_pad);
    // Draw name
    lcd.setCursor(side_pad, y_pos);
    lcd.print(current_frame.slots[i].name);
    // Clear text from the last frame if it was not overdrawn already
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
    // Remember cursor position for next frame rendering
    frame_state[i].name_end = cursor_x; 

    // Draw or clear muted indicator
    if (frame_state[i].muted != current_frame.slots[i].muted || force) {
      if (current_frame.slots[i].muted) {
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
      // Remember the mute state for next frame rendering
      frame_state[i].muted = current_frame.slots[i].muted;
    }
    
    // The volume value (0 - 100) or -1 if it should not be drawn (there is no app in this slot)
    int volume_state = (current_frame.slots[i].name[0] == '\0') ? -1 : current_frame.slots[i].volume; // negative state means do not draw
    // Drawing the volume if it has changed
    if (frame_state[i].volume != volume_state || force) {
      if (volume_state < 0) {
        // Clear the volume area as it should not be drawn (negative state)
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
        lcd.printf("%3i%%", current_frame.slots[i].volume);
      }
      // Remember the volume state for next frame rendering
      frame_state[i].volume = volume_state;
    }
    
  }
}

// Write to the display to show that the device is not connected
void Display::show_disconnected() {
  if (dc_shown) {return;} // Sort circut if already shown

  constexpr char* text = "NOT CONNECTED"; // 13 chars
  backdrop(); // Clear any existing frame

  // Draw message ~centered within selection box
  lcd.setCursor(
    ((lcd.width() - (side_pad / 3)) / 2) - (39 * text_size),
    top_pad + mid_pad + (4 * text_size)
  );
  lcd.print(text);

  dc_shown = true;
}

// Clear disconnected message
void Display::clear_disconnected() {
  if (!dc_shown) {return;} // Sort circut if already clear
  // Overwrite message with background color
  lcd.fillRect(
    ((lcd.width() - (side_pad / 3)) / 2) - (39 * text_size),
    top_pad + mid_pad + (4 * text_size),
    13 * 6 * text_size,
    9 * text_size,
    bk_color
  );
  
  dc_shown = false;
}

// Get disconnection status (var is private but may be read)
bool Display::get_dc() {
  return dc_shown;
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

    // Redo backdrop with new settings
    backdrop();
  }
}

// Screen-saver function to prevent burn-in
void Display::refresh_sweep() {
  uint16_t h = lcd.height();
  uint16_t w = lcd.width();

  // Sweep the screen with inverted colors
  for (uint16_t i = 0; i < h + 30; i++) {
    int16_t ibk_line = i;
    int16_t icolor_line = i - 15;
    int16_t bk_line = i - 30;
    if (ibk_line >= 0 && ibk_line < h) {lcd.writeFastHLine(0, ibk_line, w, ~bk_color);} // invert bk color
    if (icolor_line >= 0 && icolor_line < h) {lcd.writeFastHLine(0, icolor_line, w, ~text_color);} // invert text color
    if (bk_line >= 0 && bk_line < h) {lcd.writeFastHLine(0, bk_line, w, bk_color);} // restore bk color
  }
  // Change padding so elements are drawn in a slightly different place
  cycle_padding(); 
  // Ensure last state is resumed
  if (dc_shown) {
    dc_shown = false; // Needed to force dc message to be redrawn
    show_disconnected();
  } else {
    backdrop(false);
    render_frame(true);
  }
}
