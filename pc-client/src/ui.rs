use std::sync::mpsc::Sender;

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::*,
};

use crate::state::{AppStateHandle, Command, ConnectionStatus};
use crate::tray::AppTray;

const IDC_STATUS:    i32 = 101;
const IDC_STARTSTOP: i32 = 102;
const IDC_LOG:       i32 = 103;

const TIMER_REFRESH: usize = 1;

// Listbox messages/styles not in Win32_UI_WindowsAndMessaging
const LB_ADDSTRING:        u32 = 0x0180;
const LB_GETCOUNT:         u32 = 0x018B;
const LB_DELETESTRING:     u32 = 0x0182;
const LB_SETTOPINDEX:      u32 = 0x0197;
const LBS_NOINTEGRALHEIGHT: u32 = 0x0100;
const LBS_NOSEL:            u32 = 0x4000;
const SS_LEFT:              u32 = 0x0000;
const BS_PUSHBUTTON:        u32 = 0x0000;
const WS_EX_CLIENTEDGE:    u32 = 0x0200;

struct WindowState {
    state:        AppStateHandle,
    cmd_tx:       Sender<Command>,
    last_log_len: usize,
    status_hwnd:  HWND,
    btn_hwnd:     HWND,
    log_hwnd:     HWND,
}

pub fn run(state: AppStateHandle, cmd_tx: Sender<Command>, tray: Option<AppTray>) {
    let hwnd = create_main_window(state, cmd_tx);

    // tray-icon's TrayIcon is !Send so must live on the main thread.
    // Poll it in the message loop via a WM_TIMER instead of a thread.
    // Store tray in a thread-local so WM_TIMER handler can access it.
    MAIN_TRAY.with(|cell| {
        *cell.borrow_mut() = tray;
    });

    unsafe {
        SetTimer(hwnd, TIMER_REFRESH, 150, None);

        let mut msg = std::mem::zeroed::<MSG>();
        loop {
            let ret = GetMessageW(&mut msg, 0, 0, 0);
            if ret == 0 || ret == -1 { break; }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

std::thread_local! {
    static MAIN_TRAY: std::cell::RefCell<Option<AppTray>> = std::cell::RefCell::new(None);
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn create_main_window(state: AppStateHandle, cmd_tx: Sender<Command>) -> HWND {
    unsafe {
        let hinstance = GetModuleHandleW(std::ptr::null());
        let class_name = to_wide("PhoneMikeWnd");

        let wc = WNDCLASSEXW {
            cbSize:        std::mem::size_of::<WNDCLASSEXW>() as u32,
            style:         CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc:   Some(wnd_proc),
            cbClsExtra:    0,
            cbWndExtra:    0,
            hInstance:     hinstance,
            hIcon:         LoadIconW(0, IDI_APPLICATION),
            hCursor:       LoadCursorW(0, IDC_ARROW),
            hbrBackground: 6, // COLOR_WINDOW (5) + 1 = white background
            lpszMenuName:  std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm:       LoadIconW(0, IDI_APPLICATION),
        };
        RegisterClassExW(&wc);

        let ws = Box::new(WindowState {
            state,
            cmd_tx,
            last_log_len: 0,
            status_hwnd: 0,
            btn_hwnd: 0,
            log_hwnd: 0,
        });
        let create_param = Box::into_raw(ws) as *mut core::ffi::c_void;

        let title = to_wide("PhoneMike");
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT, CW_USEDEFAULT,
            420, 340,
            0, 0,
            hinstance,
            create_param,
        );

        ShowWindow(hwnd, SW_SHOWNORMAL);
        hwnd
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = &*(lparam as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);

            let hinstance = GetModuleHandleW(std::ptr::null());

            let status_hwnd = CreateWindowExW(
                0,
                to_wide("STATIC").as_ptr(),
                to_wide("\u{25CF} Disconnected").as_ptr(),
                WS_CHILD | WS_VISIBLE | SS_LEFT,
                10, 10, 380, 24,
                hwnd, IDC_STATUS as _, hinstance, std::ptr::null(),
            );

            let btn_hwnd = CreateWindowExW(
                0,
                to_wide("BUTTON").as_ptr(),
                to_wide("Start").as_ptr(),
                WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON,
                10, 44, 100, 28,
                hwnd, IDC_STARTSTOP as _, hinstance, std::ptr::null(),
            );

            let log_hwnd = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                to_wide("LISTBOX").as_ptr(),
                std::ptr::null(),
                WS_CHILD | WS_VISIBLE | WS_VSCROLL | LBS_NOINTEGRALHEIGHT | LBS_NOSEL,
                10, 82, 380, 210,
                hwnd, IDC_LOG as _, hinstance, std::ptr::null(),
            );

            let ws = &mut *(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState);
            ws.status_hwnd = status_hwnd;
            ws.btn_hwnd    = btn_hwnd;
            ws.log_hwnd    = log_hwnd;

            0
        }

        WM_SIZE => {
            let ws = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if ws.is_null() { return DefWindowProcW(hwnd, msg, wparam, lparam); }
            let ws = &mut *ws;

            let width  = (lparam & 0xFFFF) as i32;
            let height = ((lparam >> 16) & 0xFFFF) as i32;

            MoveWindow(ws.status_hwnd, 10, 10, width - 20, 24, 1);
            MoveWindow(ws.btn_hwnd,    10, 44, 100, 28, 1);
            MoveWindow(ws.log_hwnd,    10, 82, width - 20, height - 92, 1);
            0
        }

        WM_TIMER => {
            if wparam == TIMER_REFRESH {
                // Poll tray events on main thread (tray-icon is !Send)
                MAIN_TRAY.with(|cell| {
                    if let Some(ref tray) = *cell.borrow() {
                        let is_active = {
                            let ws = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
                            if ws.is_null() { false }
                            else { (*ws).state.lock().map(|st| st.status.is_active()).unwrap_or(false) }
                        };
                        let ev = crate::tray::poll_tray(tray, is_active);
                        if ev.quit       { PostQuitMessage(0); }
                        if ev.toggle     { toggle_window(hwnd); }
                        if ev.connect    { send_start(hwnd); }
                        if ev.disconnect { send_stop(hwnd); }
                    }
                });
                refresh_ui(hwnd);
            }
            0
        }

        WM_COMMAND => {
            let control_id = (wparam & 0xFFFF) as i32;
            if control_id == IDC_STARTSTOP {
                let ws = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
                if !ws.is_null() {
                    let ws = &mut *ws;
                    let is_active = ws.state.lock().map(|st| st.status.is_active()).unwrap_or(false);
                    if is_active {
                        let _ = ws.cmd_tx.send(Command::Stop);
                    } else {
                        let _ = ws.cmd_tx.send(Command::Start {
                            use_driver: true,
                            wav_path: None,
                            gain: 1.0,
                            noise_gate: 0.0,
                            lowpass_hz: 24000.0,
                        });
                    }
                }
            }
            0
        }

        WM_CLOSE => {
            ShowWindow(hwnd, SW_HIDE);
            0
        }

        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn toggle_window(hwnd: HWND) {
    let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
    if style & WS_VISIBLE != 0 {
        ShowWindow(hwnd, SW_HIDE);
    } else {
        ShowWindow(hwnd, SW_SHOWNORMAL);
        SetForegroundWindow(hwnd);
    }
}

unsafe fn send_start(hwnd: HWND) {
    let ws = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
    if ws.is_null() { return; }
    let _ = (*ws).cmd_tx.send(Command::Start {
        use_driver: true,
        wav_path: None,
        gain: 1.0,
        noise_gate: 0.0,
        lowpass_hz: 24000.0,
    });
}

unsafe fn send_stop(hwnd: HWND) {
    let ws = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
    if ws.is_null() { return; }
    let _ = (*ws).cmd_tx.send(Command::Stop);
}

unsafe fn refresh_ui(hwnd: HWND) {
    let ws = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
    if ws.is_null() { return; }
    let ws = &mut *ws;

    let Ok(st) = ws.state.lock() else { return };

    let status_text = match &st.status {
        ConnectionStatus::Disconnected                         => "\u{25CF} Disconnected".to_string(),
        ConnectionStatus::Connecting                           => "\u{25CF} Connecting...".to_string(),
        ConnectionStatus::Streaming { sample_rate, channels } =>
            format!("\u{25CF} Streaming {}Hz/{}ch", sample_rate, channels),
        ConnectionStatus::Error(e)                             => format!("\u{25CF} Error: {}", e),
    };
    let is_active = st.status.is_active();
    let update_tag = st.update_available.clone();

    let log_len = st.log.len();
    let new_lines: Vec<String> = if log_len > ws.last_log_len {
        st.log.iter().skip(ws.last_log_len).cloned().collect()
    } else {
        vec![]
    };
    drop(st);

    SetWindowTextW(ws.status_hwnd, to_wide(&status_text).as_ptr());

    let btn_label = if is_active { "Stop" } else { "Start" };
    SetWindowTextW(ws.btn_hwnd, to_wide(btn_label).as_ptr());

    for line in &new_lines {
        let count = SendMessageW(ws.log_hwnd, LB_GETCOUNT, 0, 0);
        if count >= 500 {
            SendMessageW(ws.log_hwnd, LB_DELETESTRING, 0, 0);
        }
        let wide = to_wide(line);
        SendMessageW(ws.log_hwnd, LB_ADDSTRING, 0, wide.as_ptr() as LPARAM);
    }

    ws.last_log_len = log_len;

    let count = SendMessageW(ws.log_hwnd, LB_GETCOUNT, 0, 0);
    if count > 0 {
        SendMessageW(ws.log_hwnd, LB_SETTOPINDEX, (count - 1) as usize, 0);
    }

    if let Some(tag) = update_tag {
        let title = format!("PhoneMike \u{2014} update {} available", tag);
        SetWindowTextW(hwnd, to_wide(&title).as_ptr());
    }
}
