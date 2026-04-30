use image::GenericImageView;
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

static ICON_32_PNG: &[u8] = include_bytes!("../../assets/icons/windows/logo_32.png");

fn make_icon() -> tray_icon::Icon {
    let img = image::load_from_memory(ICON_32_PNG)
        .expect("tray icon PNG decode")
        .into_rgba8();
    let (w, h) = img.dimensions();
    tray_icon::Icon::from_rgba(img.into_raw(), w, h).expect("tray icon build")
}
