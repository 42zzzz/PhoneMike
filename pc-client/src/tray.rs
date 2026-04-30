use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder, TrayIconEvent,
};

#[allow(dead_code)]
pub struct AppTray {
    pub icon: TrayIcon,
    // Items kept alive so we can call set_enabled each frame
    pub show_item:       MenuItem,
    pub connect_item:    MenuItem,
    pub disconnect_item: MenuItem,
    pub quit_item:       MenuItem,
    // IDs for event matching
    pub show_item_id:       tray_icon::menu::MenuId,
    pub connect_item_id:    tray_icon::menu::MenuId,
    pub disconnect_item_id: tray_icon::menu::MenuId,
    pub quit_item_id:       tray_icon::menu::MenuId,
}

/// Build tray icon + context menu. Call before eframe::run_native.
pub fn build_tray() -> anyhow::Result<AppTray> {
    let tray_menu = Menu::new();

    let show_item       = MenuItem::new("Show / Hide", true,  None);
    let connect_item    = MenuItem::new("Connect",     true,  None);
    let disconnect_item = MenuItem::new("Disconnect",  false, None); // disabled until active
    let quit_item       = MenuItem::new("Quit",        true,  None);

    tray_menu.append(&show_item)?;
    tray_menu.append(&PredefinedMenuItem::separator())?;
    tray_menu.append(&connect_item)?;
    tray_menu.append(&disconnect_item)?;
    tray_menu.append(&PredefinedMenuItem::separator())?;
    tray_menu.append(&quit_item)?;

    let show_item_id       = show_item.id().clone();
    let connect_item_id    = connect_item.id().clone();
    let disconnect_item_id = disconnect_item.id().clone();
    let quit_item_id       = quit_item.id().clone();

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_icon(make_icon())
        .with_tooltip("PhoneMike")
        .build()?;

    Ok(AppTray {
        icon: tray,
        show_item,
        connect_item,
        disconnect_item,
        quit_item,
        show_item_id,
        connect_item_id,
        disconnect_item_id,
        quit_item_id,
    })
}

pub struct TrayEvents {
    pub toggle:     bool,
    pub connect:    bool,
    pub disconnect: bool,
    pub quit:       bool,
}

/// Poll tray events. Updates Connect/Disconnect enabled state based on is_active.
pub fn poll_tray(tray: &AppTray, is_active: bool) -> TrayEvents {
    tray.connect_item.set_enabled(!is_active);
    tray.disconnect_item.set_enabled(is_active);

    let mut ev = TrayEvents { toggle: false, connect: false, disconnect: false, quit: false };

    while let Ok(event) = TrayIconEvent::receiver().try_recv() {
        if matches!(event, TrayIconEvent::Click { button: tray_icon::MouseButton::Left, .. }) {
            ev.toggle = true;
        }
    }

    while let Ok(event) = MenuEvent::receiver().try_recv() {
        if event.id == tray.show_item_id {
            ev.toggle = true;
        } else if event.id == tray.connect_item_id {
            ev.connect = true;
        } else if event.id == tray.disconnect_item_id {
            ev.disconnect = true;
        } else if event.id == tray.quit_item_id {
            ev.quit = true;
        }
    }

    ev
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
                rgba[idx]     = 50;
                rgba[idx + 1] = 200;
                rgba[idx + 2] = 80;
                rgba[idx + 3] = 255;
            } else if dist < inner_r {
                rgba[idx]     = 30;
                rgba[idx + 1] = 30;
                rgba[idx + 2] = 35;
                rgba[idx + 3] = 255;
            } else {
                rgba[idx + 3] = 0;
            }
        }
    }

    tray_icon::Icon::from_rgba(rgba, SIZE, SIZE).expect("icon build")
}
