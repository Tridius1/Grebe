# Grebe 

**Grebe** is an open-source, physical hardware volume mixer built exclusively for Windows. It bridges a custom hardware controller with a lightweight, portable Windows background daemon, allowing you to control your system's audio using physical rotary encoders. Grebe integrates with the Windows native volume mixer, allowing adjustment of the volume of individiual apps.

The project is split into two distinct parts:
1. **The Daemon:** A fast, portable background application written in Rust that runs on your Windows machine.
2. **The Controller:** Microcontroller firmware written in C++ that reads physical inputs and drives the display.

---

## Getting Started

The easiest way to get started is to use the pre-compiled binaries for both the Windows daemon and the microcontroller.

**[Download the latest compiled binaries from the Releases tab!](../../releases/latest)**

### 1. Hardware Setup
Currently, Grebe's firmware is tailored for the **ESP32** and a **DIYables LCD**. Support for other microcontrollers and displays may be added but is not currently planned.

**Required Components:**
* 1x ESP32 DEVKIT V1 (or comparable microcontroller)
* 1x DIYables LCD Display (or comparable LCD display)
* 2x EC11 Rotary Encoders (with push-button functionality)
* Jumper wires (or soldered connections)

#### Wiring Diagram
![Schematic_Grebe_2026-07-21.png](https://github.com/Tridius1/Grebe/blob/main/Schematic_Grebe_2026-07-21.png)

### 2. Installing the Windows Daemon
Grebe is a completely portable application—no traditional installers required.

1. Download the `Grebe.exe` Windows binary from the **Releases** tab.
2. Place the executable in a dedicated folder.
3. Run `Grebe.exe`. The daemon will automatically generate a `config.toml` file on startup.
4. Close Grebe from the Windows system tray and edit the generated config file.
  * The only config option it is critical to change is `port`. This must be set to match the COM port your ESP32 in connected to. If you don't know this, review the detailed instructions for flashing your ESP32 below.

### 3. Flashing the Microcontroller
#### Overview: If you have flashed a microcontroller before this should be all you need.
1. Download the compiled `.bin` file for the ESP32 from the **Releases** tab.
2. Connect your ESP32 or comparable microcontroller to your PC via USB and ensure the apropriate driver is installed.
4. Flash the binary to your ESP32 using you preferred flashing tool.

#### Detailed Instructions: If you are not experienced with microcontrollers, follow this step-by-step guide.
1. Download the compiled `.bin` file for the ESP32 from the **Releases** tab.
2. Connect your ESP32 DEVKIT V1 to your PC via USB.
3. Install the CP210x USB to UART Bridge VCP Drivers:
     1. Download the `CP210x Windows Drivers` from the [Silicon Labs Website](https://www.silabs.com/software-and-tools/usb-to-uart-bridge-vcp-drivers?tab=downloads).
     2. Unzip the compressed folder.
     3. Run the installer (`CP210xVCPInstaller_x64.exe`).
5. Identify the COM port your ESP32 is connecting to. This can be done by opening the Device Manager, navigating to Ports (COM & LPT), and looking for a device that appears only when your ESP32 is connected. Take note of the COM number (`COM3`, `COM4`, etc). You will need this later.
6. Open a terminal and navigate to the folder containing the compiled `.bin` file.
8. Ensure python is installed. You can do this with `py --version`
     * If python is not installed, install it from the [Python Website](https://www.python.org/downloads/windows/).
8. Install `esptool.py` with `pip install esptool --user`
9. Ensure the Grebe daemon is not running. It can be found and closed in the system tray.
10. Flash the microcontroller. Use the following command, but replace `COM#` with the COM port you identified in step 5.
      Ensure you execute this command in the folder containing the the compiled `.bin` file downloaded in step 1.
    
    ```bash
    py -m esptool --chip esp32 --port COM# --baud 921600 write-flash 0x0 .\Grebe_ESP32_Firmware.bin
     ```
    * If you see a `FileNotFoundError` it is likely that the port is incorrect or your microcontroller is not plugged in.
    * If you see a `PermissionError` it is likely that the Grebe daemon is open and must be closed prior to flashing.
12. Your microcontroller should be ready to use. Ensure you have set the `port` option in `config.toml` to match the COM port you identified in step 5.

---

## Building from Source

If you prefer to tinker, modify the codebase, or adapt the firmware for different microcontrollers, you can easily build Grebe from source.

### Building the Windows Daemon (Rust)
1. Ensure you have [Rust and Cargo](https://rustup.rs/) installed.
2. Clone the repository.
3. Navigate to the repository directory.
4. Use `cargo build --release`

### Building the Microcontroller Application (C++)
1. Ensure you have the [Arduino IDE](https://www.arduino.cc/en/software/) installed.
2. Clone the repository.
3. Navigate to the repository directory.
4. Open `Grebe/hardware/ESP32_DEVKITV1/ESP32_DEVKITV1.ino` with the Arduino IDE.
5. In the Arduino IDE open the Library Manager and install the following libraries with dependencies:
   1. ESP32Encoder by Kevin Harrington
   2. DIYables TFT Shield by DIYables.io
6. If you are using an ESP32 modify the DIYables TFT Shield library for use with an ESP32:
   1. Navigate to `Documents/Arduino/libraries/DIYables_TFT_Shield/src` and open `DIYables_TFT_Shield.h` in your preferred text editor.
   2. Locate this code snippet, starting on line 17:
      ```cpp
      // Control pins
      #define API_PIN_RD   A0
      #define API_PIN_WR   A1
      #define API_PIN_CD   A2
      #define API_PIN_CS   A3
      #define API_PIN_RESET  A4
      ```
      Replace the above code with the following code.
      
      ```cpp
      // Control pins
      #define API_PIN_RD   2
      #define API_PIN_WR   4
      #define API_PIN_CD   15
      #define API_PIN_CS   33
      #define API_PIN_RESET  32
      ```
7. Select you board and port in the Arduino IDE. If you are using an ESP32 you will need to add the ESP32 boards manager. Open File -> Preferences and add `https://espressif.github.io/arduino-esp32/package_esp32_index.json` to the `Additional boards manager URLs` field.
8. Use the Upload button in the Arduino IDE to flash your microcontroller.

---

## Contributing

Contributions, issues, and feature requests are welcome! If you have adapted the firmware to work with a different microcontroller or display, feel free to open a Pull Request.

## License

This project is open-source and licensed under two separate licenses depending on the asset:

* **Source Code:** All source code is licensed under the [MIT License](LICENSE).
* **Project Icon:** The Grebe project icon is a derivative work based on a photograph by **Steve Garvie**, originally uploaded to Flickr on June 21, 2010. In accordance with the original [Creative Commons Attribution-ShareAlike 2.0 Generic (CC BY-SA 2.0)](https://creativecommons.org/licenses/by-sa/2.0/) license, this derived icon is also distributed under CC BY-SA 2.0.
