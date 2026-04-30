# PhoneMike Logo Guide

Design at 1024x1024, export down. SVG source goes in `assets/logo.svg`.

## Windows

All sizes packed into a single `.ico` file.

| Size | Use | Path |
|------|-----|------|
| 16x16 | Taskbar / Explorer small | `assets/icons/windows/logo_16.png` |
| 24x24 | Taskbar medium | `assets/icons/windows/logo_24.png` |
| 32x32 | Explorer default, system tray | `assets/icons/windows/logo_32.png` |
| 48x48 | Explorer large | `assets/icons/windows/logo_48.png` |
| 64x64 | Explorer extra large | `assets/icons/windows/logo_64.png` |
| 256x256 | Explorer jumbo / installer | `assets/icons/windows/logo_256.png` |
| multi | Final ICO (all above packed) | `assets/icons/windows/logo.ico` |

Generate ICO from PNG:
```
magick assets/icons/windows/logo_256.png -define icon:auto-resize=256,64,48,32,24,16 assets/icons/windows/logo.ico
```

The `.ico` is referenced by the InnoSetup installer (`installer/phonemic-setup.iss`) and the eframe window (`pc-client/src/main.rs`).

## Android

| Size | Density | Path |
|------|---------|------|
| 48x48 | mdpi | `app/src/main/res/mipmap-mdpi/ic_launcher.png` |
| 72x72 | hdpi | `app/src/main/res/mipmap-hdpi/ic_launcher.png` |
| 96x96 | xhdpi | `app/src/main/res/mipmap-xhdpi/ic_launcher.png` |
| 144x144 | xxhdpi | `app/src/main/res/mipmap-xxhdpi/ic_launcher.png` |
| 192x192 | xxxhdpi | `app/src/main/res/mipmap-xxxhdpi/ic_launcher.png` |
| 1024x1024 | Play Store listing | `assets/icons/android/play_store_icon.png` |

Adaptive icon (API 26+): foreground + background layers, each 108x108. Safe zone = inner 72x72.

| File | Path |
|------|------|
| Foreground | `app/src/main/res/mipmap-anydpi-v26/ic_launcher_foreground.xml` |
| Background | `app/src/main/res/mipmap-anydpi-v26/ic_launcher_background.xml` |
