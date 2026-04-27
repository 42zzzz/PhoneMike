# PhoneMike

Turn your Android phone into a Windows microphone. 
Works with Discord, OBS, Teams, and anything else that uses a mic.

## [Download](https://github.com/42zzzz/PhoneMike/releases/latest)

| File | What to download |
|------|-----------------|
| `PhoneMike-v1.0.0-windows-setup.exe` | Install this on your PC |
| `PhoneMike.apk` | Install this on your phone |

---

## Setup

### Step 1: One-time Windows setting

PhoneMike uses a custom audio driver that requires test signing to be enabled. Open **Command Prompt as Administrator** and run:

```
bcdedit /set testsigning on
```

Then **restart your PC**.

> Done only once. A small "Test Mode" watermark might appear on your desktop; this is normal.

### Step 2: Install on your PC

Run `PhoneMike-v1.0.0-windows-setup.exe`. When asked, tick **"Install virtual microphone driver"**.

After install, **PhoneMike Virtual Microphone** will appear as a recording device in Windows Sound Settings.

### Step 3: Phone Installation

Your phone needs **USB debugging** enabled:
1. Go to **Settings → About phone**
2. Tap **Build number** 7 times to unlock Developer Options
3. Go to **Settings → Developer Options** and turn on **USB debugging**

Then install `PhoneMike.apk` by copying it to your phone and opening it (you may need to allow installs from unknown sources).

### Step 4: Usage

1. Plug your phone into your PC with a USB cable
2. Run this command once (every time you plug in):
   ```
   adb forward tcp:18501 tcp:18501
   ```
3. Open the **PhoneMike app** on your phone and tap **Start**
4. Open **PhoneMike Client** on your PC (Start Menu or desktop shortcut)
5. In Discord / OBS / Teams, select **PhoneMike Virtual Microphone** as your input

---

## Features

- **Noise gate**: cuts background silence automatically
- **Lowpass filter**: reduces high-frequency noise
- **Opus audio codec**: compressed audio for a cleaner stream
- Works over USB cable (no Wi-Fi needed)

---

## License(LICENSE)
