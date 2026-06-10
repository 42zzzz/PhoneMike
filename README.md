# PhoneMike

Turn your Android phone into a Windows microphone.
Works with Discord, OBS, Teams, and anything else that uses a mic.

## [Download](https://github.com/42zzzz/PhoneMike/releases/latest)

| File | What to download |
|------|-----------------|
| [`PhoneMike-v1.3.0-windows-setup.exe`](https://github.com/42zzzz/PhoneMike/releases/download/v1.3.0/PhoneMike-v1.3.0-windows-setup.exe) | Install this on your PC |
| [`PhoneMike.apk`](https://github.com/42zzzz/PhoneMike/releases/download/v1.3.0/PhoneMike.apk) | Install this on your phone |

---

## Setup

### Step 0: Install a virtual audio cable

PhoneMike routes audio through a **virtual audio cable** — this is what creates the microphone that your apps will see.

**Option A (recommended — free):** [VB-Cable Virtual Audio Device](https://vb-audio.com/Cable/)

> Download the free version from vb-audio.com, run the installer, and reboot.

**Option B:** [VoiceMeeter](https://vb-audio.com/Voicemeeter/) (free, more features)

After installing, you will have a new audio device like **"CABLE Input"** (playback) and **"CABLE Output"** (recording/microphone).

### Step 1: Install PhoneMike on your PC

Run `PhoneMike-v1.3.0-windows-setup.exe`.

> No driver installation needed — no test signing, no reboot required.

### Step 2: Phone setup

Your phone needs **USB debugging** enabled:
1. Go to **Settings → About phone**
2. Tap **Build number** 7 times to unlock Developer Options
3. Go to **Settings → Developer Options** and turn on **USB debugging**

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

## Features

- **Noise gate**: cuts background silence automatically
- **Lowpass filter**: reduces high-frequency noise
- **Opus audio codec**: compressed audio for a cleaner stream
- Works over USB cable (no Wi-Fi needed)
- **No kernel driver** — compatible with anti-cheat and secure boot

---

## [License](https://github.com/42zzzz/PhoneMike/blob/main/LICENSE)
