#ifndef DIY_TFT_LCD
#define DIY_TFT_LCD

#include <DIYables_TFT_Shield.h>

struct __attribute__((packed)) FrameEntry {
    uint8_t volume;
    bool muted; // boolean
    char name[24];
};

struct __attribute__((packed)) DisplayFrame {
    FrameEntry slots[3]; // [0] = Prev, [1] = Curr, [2] = Next
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

    // Cursor positions
    const uint16_t prev_cursor[2] = { 10, 80 };
    const uint16_t curr_cursor[2] = { 10, 160 };
    const uint16_t next_cursor[2] = { 10, 240 };

    // Padding
    const uint16_t top_pad = 80;
    const uint16_t mid_pad = 80; // this is from top of text to top of tex; it does not account for text height
    const uint16_t left_pad = 10;

    // Number of chars to print to the screen
    uint8_t text_len;

    void setup();

  public:
    Display(uint8_t rot);

    void render_frame(DisplayFrame);
};

void initScreen();

#endif