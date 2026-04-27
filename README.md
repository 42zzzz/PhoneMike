# PhoneMic

Turn your Android phone into a Windows microphone over USB. Works with Discord, VoiceMeeter, OBS, and any app that uses a mic.

```
Phone mic -> Android app -> ADB/USB -> PC client -> shared memory -> WDM driver -> Windows virtual mic
```

## Download

Get the latest release from [GitHub Releases](https://github.com/42zzzz/PhoneMic/releases):

| File | What |
|------|------|
| `PhoneMic-v1.0.0-windows-setup.exe` | Windows installer (PC client + virtual mic driver) |
| `PhoneMic-v1.0.0-android.apk` | Android app (sideload via ADB) |

## Quick Start

### 1. Enable test signing (one-time, reboot required)

The virtual microphone driver requires Windows test signing mode:

```powershell
# Run as Administrator
bcdedit /set testsigning on
# Reboot
```

### 2. Install on Windows

Run `PhoneMic-v1.0.0-windows-setup.exe`. Check "Install virtual microphone driver" during setup.

After install, "PhoneMic Virtual Microphone" appears in Sound Settings > Recording.

### 3. Install on Android

Enable USB debugging on your phone, then:

```bash
adb install PhoneMic-v1.0.0-android.apk
```

### 4. Use it

1. Plug phone into PC via USB
2. Open PhoneMic app on phone, tap **Start**
3. Launch **PhoneMic Client** from Start Menu (or desktop shortcut)
4. Select your phone as input in Discord/OBS/etc.

## How It Works

```
+---------------+    TCP/ADB or USB AOA    +----------------+    file-backed    +------------------+
|  Android App  | -----------------------> |   PC Client    | ----ring.dat----> |   WDM Driver     |
|  (mic capture)|    PHMC protocol         |   (Rust/egui)  |   shared mem     |  (virtual mic)   |
+---------------+                          +----------------+                   +------------------+
```

- **Android app** captures mic audio via AudioRecord (48kHz, mono, 16-bit PCM)
- **PC client** reads audio over TCP/ADB, writes to `C:\ProgramData\PhoneMic\ring.dat`
- **WDM driver** maps that file into kernel space, feeds PCM to Windows audio stack
- Apps see "PhoneMic Virtual Microphone" as a standard recording device

Two transport modes:
- **TCP/ADB** (default) — plug in USB, works automatically via `adb forward`
- **USB AOA** — direct USB accessory mode, requires WinUSB driver via [Zadig](https://zadig.akeo.ie/)

## CLI Options

```
phonemic-client [OPTIONS]

Options:
  -o, --output <FILE>    Dump audio to WAV file
  -d, --duration <SECS>  Capture duration (0 = until Ctrl-C)
      --buf-size <N>     Read buffer size in bytes [default: 4096]
      --usb              Use USB AOA transport (requires WinUSB/Zadig)
      --negotiate        Negotiate AOA mode (USB only)
      --driver           Write to shared memory for kernel driver
```

## Building from Source

### Android App
```bash
./gradlew assembleDebug
adb install app/build/outputs/apk/debug/app-debug.apk
```

### PC Client
```bash
cd pc-client
cargo build --release
# Output: target/release/phonemic-client.exe
```

### WDM Driver
Requires WDK 10.0.26100.0 and Visual Studio with v143 toolset.

```powershell
cd driver
powershell -ExecutionPolicy Bypass -File rebuild.ps1
# Output: x64/Debug/PhoneMicDriver.sys

# Install (admin, test signing must be on):
powershell -ExecutionPolicy Bypass -File install.ps1
```

### Windows Installer
Requires [Inno Setup 6](https://jrsoftware.org/isinfo.php). Build all components first, then:

```powershell
& "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" installer\phonemic-setup.iss
# Output: installer/Output/PhoneMic-v1.0.0-windows-setup.exe
```

## Project Structure

```
app/                          Android app (Kotlin, Jetpack Compose)
  src/main/java/.../
    service/AudioService.kt   Foreground service, audio capture
    tcp/TcpAudioServer.kt     TCP server for ADB transport
    usb/UsbController.kt      USB AOA lifecycle
    ui/MainScreen.kt          Compose UI

pc-client/                    Rust PC client
  src/main.rs                 Entry point
  src/audio_thread.rs         Audio read loop, shared mem writer
  src/tcp.rs                  TCP/ADB transport
  src/usb.rs                  USB AOA transport
  src/shared_mem.rs           File-backed shared memory writer
  src/app.rs                  egui GUI

driver/                       Windows WDM kernel driver
  src/driver_entry.cpp        DriverEntry, AddDevice, StartDevice
  src/miniport.cpp            IMiniportWaveCyclic (capture-only)
  src/stream.cpp              DMA buffer, worker thread, ServiceBuffer
  src/shared_mem.cpp          Kernel-mode ring buffer reader
  src/dma_channel.cpp         Virtual DMA channel (no hardware DMA)
  phonemic.inf                Driver INF
  install.ps1                 Test-sign + install script
  rebuild.ps1                 Build script (MSBuild + manual link)

installer/                    InnoSetup installer
  phonemic-setup.iss          Installer script
```

## Audio Format

- 48 kHz, mono, 16-bit signed PCM
- PHMC header (16 bytes LE) sent on connection: magic `"PHMC"`, sample rate, channels, format, frame size
- Shared memory ring buffer: 64KB file at `C:\ProgramData\PhoneMic\ring.dat` (28-byte header + 65508 bytes ring data)

## License

See [LICENSE](LICENSE).
