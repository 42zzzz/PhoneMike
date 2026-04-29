// Suppress console window in GUI mode. Headless reattaches via AttachConsole.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod audio_thread;
mod shared_mem;
mod state;
mod tcp;
mod tray;
mod update;
mod wav;

use std::sync::{mpsc, Arc, Mutex};

use clap::Parser;
use eframe::egui;

use state::{Command, SharedState};

#[derive(Parser, Debug)]
#[command(about = "PhoneMike PC client \u{2014} receives PCM audio from Android")]
struct Args {
    /// Run without GUI (original headless CLI behavior)
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
    } else {
        run_gui();
    }
}

fn run_gui() {
    let state = Arc::new(Mutex::new(SharedState::new()));
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();

    let state_clone = Arc::clone(&state);
    std::thread::spawn(move || {
        audio_thread::run_audio_thread(cmd_rx, state_clone);
    });

    update::spawn_update_check(Arc::clone(&state));

    // Build tray icon before run_native (must be on main thread)
    let app_tray = tray::build_tray().ok();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("PhoneMike")
            .with_inner_size([900.0, 620.0])
            .with_min_inner_size([700.0, 450.0]),
        ..Default::default()
    };

    eframe::run_native(
        "PhoneMike",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(app::PhoneMikeApp::new(cc, Arc::clone(&state), cmd_tx, app_tray)))
        }),
    )
    .expect("eframe failed to start");
}

fn run_headless(args: Args) {
    // Reattach to parent terminal so eprintln! output is visible
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