#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod audio_thread;
mod shared_mem;
mod state;
mod tcp;
mod tray;
mod ui;
mod update;
mod wav;

use std::sync::{mpsc, Arc, Mutex};

use clap::Parser;

use state::{Command, SharedState};

#[derive(Parser, Debug)]
#[command(about = "PhoneMike PC client \u{2014} receives PCM audio from Android")]
struct Args {
    /// Run without GUI (headless CLI behavior)
    #[arg(long, default_value_t = false)]
    headless: bool,

    /// Dump received audio to WAV file (e.g. output.wav)
    #[arg(short, long)]
    output: Option<String>,

    /// Seconds to capture before stopping (0 = run until Ctrl-C)
    #[arg(short, long, default_value_t = 0)]
    duration: u64,

    /// Read buffer size in bytes (default: 4096)
    #[arg(long, default_value_t = 4096)]
    buf_size: usize,

    /// Output PCM to PhoneMike virtual speaker via WASAPI
    #[arg(long, default_value_t = false)]
    driver: bool,
}

fn main() {
    let args = Args::parse();

    if args.headless {
        run_headless(args);
        return;
    }

    // Single-instance guard: named mutex prevents multiple GUI instances.
    // If already running, bring the existing window to front and exit.
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::ERROR_ALREADY_EXISTS;
        use windows_sys::Win32::System::Threading::CreateMutexW;

        let name: Vec<u16> = "PhoneMike_SingleInstance\0".encode_utf16().collect();
        let hmutex = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
        if hmutex == 0 || unsafe { windows_sys::Win32::Foundation::GetLastError() } == ERROR_ALREADY_EXISTS {
            // Another instance is running — find its window and show it
            unsafe {
                let class: Vec<u16> = "PhoneMikeWnd\0".encode_utf16().collect();
                let hwnd = windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW(
                    class.as_ptr(), std::ptr::null(),
                );
                if hwnd != 0 {
                    windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
                        hwnd,
                        windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
                    );
                    windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
                }
            }
            return;
        }
        // Leak hmutex — held for process lifetime, released on exit by OS
        std::mem::forget(hmutex);
    }

    run_gui();
}

fn run_gui() {
    let state = Arc::new(Mutex::new(SharedState::new()));
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();

    let state_audio = Arc::clone(&state);
    std::thread::spawn(move || {
        audio_thread::run_audio_thread(cmd_rx, state_audio);
    });

    update::spawn_update_check(Arc::clone(&state));

    let app_tray = tray::build_tray().ok();

    ui::run(state, cmd_tx, app_tray);
}

fn run_headless(args: Args) {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        AttachConsole(ATTACH_PARENT_PROCESS);
    }

    let state = Arc::new(Mutex::new(SharedState::new()));
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();

    cmd_tx
        .send(Command::Start {
            use_driver: args.driver,
            wav_path: args.output.clone(),
            gain: 1.0,
            noise_gate: 0.0,
            lowpass_hz: 24000.0,
        })
        .unwrap();

    if args.duration > 0 {
        let tx = cmd_tx.clone();
        let dur = args.duration;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(dur));
            let _ = tx.send(Command::Stop);
        });
    }

    audio_thread::run_audio_thread(cmd_rx, state);
}
