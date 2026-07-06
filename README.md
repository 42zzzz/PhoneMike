# PhoneMike

Turn your Android phone into a Windows microphone.
Works with Discord, OBS, Teams, and anything else that uses a mic.

## [Download](https://github.com/42zzzz/PhoneMike/releases/latest)

| File                                                                                                                                    | What to download           |
| --------------------------------------------------------------------------------------------------------------------------------------- | -------------------------- |
| [`PhoneMike-v1.3.0-windows-setup.exe`](https://github.com/42zzzz/PhoneMike/releases/download/v1.3.0/PhoneMike-v1.3.0-windows-setup.exe) | Install this on your PC    |
| [`PhoneMike.apk`](https://github.com/42zzzz/PhoneMike/releases/download/v1.3.0/PhoneMike.apk)                                           | Install this on your phone |

---

## Setup

### Step 0: Install a virtual audio cable

PhoneMike routes audio through a **virtual audio cable** - this is what creates the microphone that your apps will see.

**Option A (recommended - free):** [VB-Cable Virtual Audio Device](https://vb-audio.com/Cable/)

> Download the free version from vb-audio.com, run the installer, and reboot.

**Option B:** [VoiceMeeter](https://vb-audio.com/Voicemeeter/) (free, more features)

After installing, you will have a new audio device like **"CABLE Input"** (playback) and **"CABLE Output"** (recording/microphone).

### Step 1: Install PhoneMike on your PC

Run `PhoneMike-v1.3.0-windows-setup.exe`.

> No driver installation/test signing/reboot required.

### Step 2: Phone setup

Your phone needs **USB debugging** enabled:

1. Go to **Settings -> About phone**
2. Tap **Build number** 7 times to unlock Developer Options
3. Go to **Settings -> Developer Options** and turn on **USB debugging**

Then install `PhoneMike.apk` by copying it to your phone and opening it (you may need to allow installs from unknown sources).

### Step 3: Usage

1. Plug your phone into your PC with a USB cable
2. Run this command once (every time you plug in):
   ```
   adb forward tcp:18501 tcp:18501
   ```
3. Open the **PhoneMike app** on your phone and tap **Start**
4. Open **PhoneMike Client** on your PC (Start Menu or desktop shortcut)
5. In Discord / OBS / Teams, select **CABLE Output (VB-Audio Virtual Cable)** as your input device

---

## How It Works

PhoneMike is a two-component system connected over USB via ADB port forwarding:

```
[Android Phone]              [Windows PC]
   AudioRecord       TCP         Decode + DSP
   -> Opus encoder   --->       -> play into
   -> TCP server     :18501        VB-Cable virtual device
                                     |
                                  Any app sees
                                  "CABLE Output" as mic
```

- **Android side** captures 48 kHz / mono / 16-bit PCM via AudioRecord, optionally encodes it with Opus (24 kbps), and streams it over TCP on port 18501.
- **PC side** connects via ADB port forwarding, decodes the stream (Opus or raw PCM), applies a DSP chain (gain, lowpass filter, noise gate), and plays the audio into a VB-Cable or VoiceMeeter virtual device. Any application can then select that virtual device as its microphone input.

Both components are designed as standalone executables with no framework runtime dependencies.

---

## Why No Kernel Driver?

Older versions of PhoneMike used a custom WDF kernel driver to create the virtual audio device on Windows. This required:

- **Test signing mode** to be enabled (BCDEdit)
- Bypassing Secure Boot on many systems
- Compatibility issues with anti-cheat kernel drivers (EAC, BattlEye, Vanguard, etc.) that refuse to run alongside test-signed or unsigned drivers

The architecture was redesigned to eliminate the kernel driver entirely. Instead, PhoneMike plays audio into an existing userspace virtual audio cable (VB-Cable or VoiceMeeter). This means:

- No test signing required
- Fully compatible with Secure Boot
- Works with all games and anti-cheat systems
- Clean uninstall with no leftover driver artifacts

---

## Tech Stack

| Component   | Language    | Frameworks / Libraries                                                                                             |
| ----------- | ----------- | ------------------------------------------------------------------------------------------------------------------ |
| Android App | Kotlin      | Jetpack Compose (Material 3), AndroidX Lifecycle + ViewModel, Kotlin Coroutines, AudioRecord API                   |
| PC Client   | Rust        | cpal (WASAPI audio output), audiopus (Opus decoding), tray-icon (system tray), clap (CLI), windows-sys (Win32 API) |
| Audio Codec | C (via JNI) | Opus (bundled as opus.aar from theeasiestway/android-opus-codec)                                                   |
| Installer   | InnoSetup 6 | --                                                                                                                 |

### Key Architecture Decisions

- **Pure Win32 GUI on PC** - no Electron, no webview. The PC client uses raw Win32 API calls via windows-sys, keeping the binary small and startup instant.
- **ADB as transport** - leverages the existing Android Debug Bridge for USB communication instead of implementing a custom USB protocol or requiring Wi-Fi.
- **Opus as optional codec** - encoding is attempted at startup; falls back transparently to raw PCM if the native library fails to load.

---

## Build from Source

### Prerequisites

- Android SDK (API 36) + JDK 11+ for the Android app
- Rust toolchain (rustup) for the PC client
- InnoSetup 6 for the Windows installer (optional)

### Android App

```bash
git clone https://github.com/42zzzz/PhoneMike.git
cd PhoneMike
# Place opus.aar in app/libs/ (from theeasiestway/android-opus-codec)
./gradlew :app:assembleRelease
```

APK will be at `app/build/outputs/apk/release/app-release.apk`.

### PC Client

```bash
cd pc-client
cargo build --release
```

Binary will be at `pc-client/target/release/PhoneMike.exe`.

### Windows Installer

Build the PC client first, then open `installer/phonemic-setup.iss` with InnoSetup and compile.

---

## Features

- **Noise gate**: cuts background silence automatically
- **Lowpass filter**: reduces high-frequency noise
- **Opus audio codec**: compressed audio for reduced bandwidth
- Works over USB cable (no Wi-Fi needed)
- **No kernel driver** - compatible with anti-cheat and secure boot

---

## Acknowledgments

This project was developed with the assistance of AI (DeepSeek V4) code generation tools, since initial prototype to architecture overhaul.

---

## [License](https://github.com/42zzzz/PhoneMike/blob/main/LICENSE)
