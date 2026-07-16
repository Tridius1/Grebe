#ifndef DIY_TFT_LCD
#define DIY_TFT_LCD

#include <DIYables_TFT_Shield.h>
#include <Preferences.h>

struct __attribute__((packed)) DisplayConfig {
  bool invert;
  uint16_t text_color;
  uint16_t bk_color;
};

struct __attribute__((packed)) FrameEntry {
    uint8_t volume;
    bool muted;
    char name[20];
};

struct __attribute__((packed)) DisplayFrame {
    FrameEntry slots[3]; // [0] = Prev, [1] = Curr, [2] = Next
};

struct LineState {
  int16_t name_end = 0;
  bool muted = false;
  int8_t volume = -1; // -1 means don't draw the %
};

class Display {
  private:
    DIYables_TFT_ILI9486_Shield lcd; // the library obj used to control the display
    // Holds the current frame
    DisplayFrame current_frame;
    bool frame_exist = false; // Has the current_frame field been populated?
    // Draw settings
    uint8_t rotation;
    uint8_t text_size;
    // Colors
    uint16_t bk_color;
    uint16_t text_color;

    // Padding
    uint16_t true_top_pad;
    uint16_t top_pad;
    uint16_t mid_pad; // this is from top of text to top of tex; it does not account for text height
    uint16_t true_side_pad;
    uint16_t side_pad;
    // Where in the padding cycle we sit; this cycles from 0 to 3
    uint8_t pad_cycle = 0;

    // Number of chars to print to the screen
    uint8_t text_len;

    // Last state, for efficient printing
    LineState frame_state[3] = {};

    // Is a disconnected messege on screen, so we know to clear it
    bool dc_shown = false;

    // Persistant storage for setings
    Preferences stored;
    
    void backdrop(bool fill_bk = true);
    void cycle_padding();

  public:
    Display();
    // Public drawing functions
    void set_frame(DisplayFrame);
    void render_frame(bool render_all = false); // draw the current frame
    void show_disconnected();
    void clear_disconnected();
    void apply_settings(DisplayConfig);
    void refresh_sweep(); // Screen-saver function to prevent burn-in
};

void initScreen();

#endif