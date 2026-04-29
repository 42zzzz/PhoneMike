use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder, TrayIconEvent,
};

#[allow(dead_code)]
pub struct AppTray {
    pub icon: TrayIcon,  // must be kept alive — dropping removes the tray icon
    pub show_item_id: tray_icon::menu::MenuId,
    pub quit_item_id: tray_icon::menu::MenuId,
}

/// Build tray icon + context menu. Call before eframe::run_native.
pub fn build_tray() -> anyhow::Result<AppTray> {
    let tray_menu = Menu::new();

    let show_item = MenuItem::new("Show / Hide", true, None);
    let quit_item = MenuItem::new("Quit", true, None);

    tray_menu.append(&show_item)?;
    tray_menu.append(&PredefinedMenuItem::separator())?;
    tray_menu.append(&quit_item)?;

    let show_item_id = show_item.id().clone();
    let quit_item_id = quit_item.id().clone();

    // Generate a simple 32×32 microphone icon (green circle on dark bg)
    let icon = make_icon();

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_icon(icon)
        .with_tooltip("PhoneMike")
        .build()?;

    Ok(AppTray { icon: tray, show_item_id, quit_item_id })
}

/// Poll tray events. Returns (toggle_visibility, quit_requested).
pub fn poll_tray(tray: &AppTray) -> (bool, bool) {
    let mut toggle = false;
    let mut quit = false;

    // Left-click on tray icon → toggle
    while let Ok(event) = TrayIconEvent::receiver().try_recv() {
        if matches!(event, TrayIconEvent::Click { button: tray_icon::MouseButton::Left, .. }) {
            toggle = true;
        }
    }

    // Menu items
    while let Ok(event) = MenuEvent::receiver().try_recv() {
        if event.id == tray.show_item_id {
            toggle = true;
        } else if event.id == tray.quit_item_id {
            quit = true;
        }
    }

    (toggle, quit)
}

fn make_icon() -> tray_icon::Icon {
    const SIZE: u32 = 32;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];

    let cx = SIZE as f32 / 2.0;
    let cy = SIZE as f32 / 2.0;
    let outer_r = 14.0f32;
    let inner_r = 8.0f32;

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let dist = (dx * dx + dy * dy).sqrt();

            let idx = ((y * SIZE + x) * 4) as usize;

            if dist <= outer_r && dist >= inner_r {
                // Green ring (microphone capsule outline)
                rgba[idx]     = 50;
                rgba[idx + 1] = 200;
                rgba[idx + 2] = 80;
                rgba[idx + 3] = 255;
            } else if dist < inner_r {
                // Dark fill inside
                rgba[idx]     = 30;
                rgba[idx + 1] = 30;
                rgba[idx + 2] = 35;
                rgba[idx + 3] = 255;
            } else {
                // Transparent outside
                rgba[idx + 3] = 0;
            }
        }
    }

    tray_icon::Icon::from_rgba(rgba, SIZE, SIZE).expect("icon build")
}