#ifndef DIY_TFT_LCD
#define DIY_TFT_LCD

#include <DIYables_TFT_Shield.h>

struct __attribute__((packed)) FrameEntry {
    uint8_t volume;
    bool muted; // boolean
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
    // Draw settings
    uint8_t rotation;
    uint8_t text_size;
    // Colors
    uint16_t bk_color;
    uint16_t text_color;

    // Padding
    uint16_t top_pad;
    uint16_t mid_pad; // this is from top of text to top of tex; it does not account for text height
    uint16_t side_pad;

    // Number of chars to print to the screen
    uint8_t text_len;

    // Last state, for efficient printing
    LineState frame_state[3] = {};

    void setup();

  public:
    Display(uint8_t rot);

    void render_frame(DisplayFrame);
};

void initScreen();

#endif