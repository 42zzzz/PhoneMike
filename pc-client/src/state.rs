use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub const MAX_LOG_LINES: usize = 500;
pub const GRAPH_HISTORY: usize = 200;

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Streaming { sample_rate: u32, channels: u16 },
    Error(String),
}

impl ConnectionStatus {
    pub fn is_active(&self) -> bool {
        matches!(self, ConnectionStatus::Connecting | ConnectionStatus::Streaming { .. })
    }
}

#[derive(Debug)]
pub enum Command {
    Start {
        use_driver: bool,
        wav_path: Option<String>,
        gain: f32,
        noise_gate: f32,
        lowpass_hz: f32,
    },
    Stop,
    SetGain(f32),
    SetNoiseGate(f32),
    SetLowpass(f32),
    StartWav(String),
    StopWav,
}

#[derive(Debug, Clone)]
pub struct AudioStats {
    pub bytes_received: u64,
    pub bytes_dropped: u64,
    pub elapsed_secs: f64,
    pub shm_write_idx: i32,
    pub shm_read_idx: i32,
    pub rms: f32,
    pub rms_history: VecDeque<f32>,
    pub driver_active: bool,
    pub gate_active: bool,
}

impl Default for AudioStats {
    fn default() -> Self {
        AudioStats {
            bytes_received: 0,
            bytes_dropped: 0,
            elapsed_secs: 0.0,
            shm_write_idx: 0,
            shm_read_idx: 0,
            rms: 0.0,
            rms_history: VecDeque::with_capacity(GRAPH_HISTORY),
            driver_active: false,
            gate_active: false,
        }
    }
}

pub struct SharedState {
    pub status: ConnectionStatus,
    pub stats: AudioStats,
    pub log: VecDeque<String>,
    /// Set by background update check when a newer GitHub release is found.
    pub update_available: Option<String>,
}

impl SharedState {
    pub fn new() -> Self {
        SharedState {
            status: ConnectionStatus::Disconnected,
            stats: AudioStats::default(),
            log: VecDeque::with_capacity(MAX_LOG_LINES),
            update_available: None,
        }
    }

    pub fn push_log(&mut self, msg: impl Into<String>) {
        if self.log.len() >= MAX_LOG_LINES {
            self.log.pop_front();
        }
        self.log.push_back(msg.into());
    }
}

pub type AppStateHandle = Arc<Mutex<SharedState>>;