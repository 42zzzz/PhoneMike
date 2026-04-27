use std::borrow::Cow;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use audiopus::coder::Decoder as OpusDecoder;
use audiopus::{Channels, SampleRate};
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Cursor;

use crate::shared_mem::SharedMemWriter;
use crate::state::{
    AppStateHandle, AudioStats, Command, ConnectionStatus, GRAPH_HISTORY,
};
use crate::tcp::TcpTransport;
use crate::wav::WavWriter;

const HEADER_SIZE: usize = 16;
const MAGIC: &[u8; 4] = b"PHMC";

// PHMC format field: 1=PCM16, 2=Opus
const FMT_OPUS: u16 = 2;

// Noise gate: keep gate open this many ms after last above-threshold chunk.
const GATE_HOLD_MS: u64 = 80;

// Max Opus frame bytes (120ms @ 510kbps)
const MAX_OPUS_FRAME_BYTES: usize = 7680;

struct PcmHeader {
    sample_rate: u32,
    channels: u16,
    format: u16,
}

fn parse_header(buf: &[u8]) -> Result<PcmHeader> {
    if buf.len() < HEADER_SIZE {
        bail!("Header too short: {} bytes", buf.len());
    }
    if &buf[0..4] != MAGIC {
        bail!("Bad magic: {:?}", &buf[0..4]);
    }
    let mut c = Cursor::new(&buf[4..]);
    let sample_rate = c.read_u32::<LittleEndian>()?;
    let channels = c.read_u16::<LittleEndian>()?;
    let format = c.read_u16::<LittleEndian>()?;
    Ok(PcmHeader { sample_rate, channels, format })
}

fn compute_rms(pcm_bytes: &[u8]) -> f32 {
    if pcm_bytes.len() < 2 {
        return 0.0;
    }
    let sum: f64 = pcm_bytes
        .chunks_exact(2)
        .map(|b| {
            let s = i16::from_le_bytes([b[0], b[1]]) as f64;
            s * s
        })
        .sum();
    let count = pcm_bytes.len() / 2;
    (sum / count as f64).sqrt() as f32 / i16::MAX as f32
}

/// Single-pole lowpass IIR filter for interleaved i16 LE PCM.
struct LowpassFilter {
    alpha: f32,
    state: Vec<f32>,
}

impl LowpassFilter {
    fn new(cutoff_hz: f32, sample_rate: u32, channels: u16) -> Self {
        LowpassFilter {
            alpha: Self::alpha(cutoff_hz, sample_rate),
            state: vec![0.0; channels as usize],
        }
    }

    fn alpha(cutoff_hz: f32, sample_rate: u32) -> f32 {
        if cutoff_hz <= 0.0 || cutoff_hz >= sample_rate as f32 / 2.0 {
            return 1.0;
        }
        let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz);
        let dt = 1.0 / sample_rate as f32;
        dt / (rc + dt)
    }

    fn update_cutoff(&mut self, cutoff_hz: f32, sample_rate: u32) {
        self.alpha = Self::alpha(cutoff_hz, sample_rate);
    }

    fn is_bypass(&self) -> bool {
        (self.alpha - 1.0).abs() < 1e-6
    }

    fn process(&mut self, buf: &mut [u8]) {
        if self.is_bypass() { return; }
        let ch = self.state.len();
        for (frame_idx, chunk) in buf.chunks_exact_mut(2).enumerate() {
            let ch_idx = frame_idx % ch;
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]) as f32;
            let y = self.state[ch_idx] + self.alpha * (sample - self.state[ch_idx]);
            self.state[ch_idx] = y;
            let out = y.clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            let b = out.to_le_bytes();
            chunk[0] = b[0];
            chunk[1] = b[1];
        }
    }
}

fn log(state: &AppStateHandle, msg: impl Into<String>) {
    let s = msg.into();
    eprintln!("{s}");
    if let Ok(mut st) = state.lock() {
        st.push_log(s);
    }
}

pub fn run_audio_thread(cmd_rx: Receiver<Command>, state: AppStateHandle) {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL,
        };
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL);
    }

    loop {
        let cmd = match cmd_rx.recv() {
            Ok(c) => c,
            Err(_) => return,
        };

        let (use_driver, initial_wav_path, initial_gain, initial_gate, initial_lowpass) = match cmd {
            Command::Start { use_driver, wav_path, gain, noise_gate, lowpass_hz } => {
                (use_driver, wav_path, gain, noise_gate, lowpass_hz)
            }
            _ => continue,
        };

        if let Err(e) = stream_session(
            &cmd_rx,
            Arc::clone(&state),
            use_driver,
            initial_wav_path,
            initial_gain,
            initial_gate,
            initial_lowpass,
        ) {
            let msg = format!("Session error: {e:#}");
            if let Ok(mut st) = state.lock() {
                st.push_log(msg.clone());
                st.status = ConnectionStatus::Error(msg);
            }
        }
    }
}

/// Read exactly `buf.len()` bytes from source.
fn read_exact(source: &TcpTransport, buf: &mut [u8]) -> Result<()> {
    let mut got = 0;
    while got < buf.len() {
        let n = source.read(&mut buf[got..], 2000).context("read_exact")?;
        got += n;
    }
    Ok(())
}

fn stream_session(
    cmd_rx: &Receiver<Command>,
    state: AppStateHandle,
    use_driver: bool,
    initial_wav_path: Option<String>,
    initial_gain: f32,
    initial_gate: f32,
    initial_lowpass: f32,
) -> Result<()> {
    {
        let mut st = state.lock().unwrap();
        st.status = ConnectionStatus::Connecting;
        st.push_log("Connecting via ADB/TCP...".to_string());
    }

    log(&state, "[audio] Setting up ADB forward...");
    let port = TcpTransport::setup_forward(18501).context("ADB forward failed")?;
    log(&state, "[audio] Waiting for phone app...");

    let mut attempt = 0u32;
    let source = loop {
        match cmd_rx.try_recv() {
            Ok(Command::Stop) => {
                log(&state, "[audio] Cancelled.");
                if let Ok(mut st) = state.lock() {
                    st.status = ConnectionStatus::Disconnected;
                }
                return Ok(());
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return Ok(()),
            _ => {}
        }

        match TcpTransport::try_connect(port)? {
            Some(t) => break t,
            None => {
                attempt += 1;
                if attempt == 1 || attempt % 10 == 0 {
                    log(&state, format!(
                        "[audio] Phone not ready, retrying... (attempt {})", attempt
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
    };
    log(&state, "[audio] TCP connected.");

    log(&state, "[audio] Reading PHMC header...");
    let mut header_buf = [0u8; HEADER_SIZE];
    read_exact(&source, &mut header_buf)?;
    let header = parse_header(&header_buf)?;

    log(&state, format!(
        "[audio] Stream: {}Hz {}ch format={}",
        header.sample_rate, header.channels,
        if header.format == FMT_OPUS { "Opus" } else { "PCM16" }
    ));

    {
        let mut st = state.lock().unwrap();
        st.status = ConnectionStatus::Streaming {
            sample_rate: header.sample_rate,
            channels: header.channels,
        };
        st.stats = AudioStats::default();
        st.stats.driver_active = use_driver;
    }

    // Build Opus decoder if stream is Opus
    let opus_sr = match header.sample_rate {
        8000  => Some(SampleRate::Hz8000),
        12000 => Some(SampleRate::Hz12000),
        16000 => Some(SampleRate::Hz16000),
        24000 => Some(SampleRate::Hz24000),
        48000 => Some(SampleRate::Hz48000),
        _     => None,
    };
    let opus_ch = match header.channels {
        1 => Some(Channels::Mono),
        2 => Some(Channels::Stereo),
        _ => None,
    };
    let mut opus_decoder: Option<OpusDecoder> = if header.format == FMT_OPUS {
        match (opus_sr, opus_ch) {
            (Some(sr), Some(ch)) => {
                let dec = OpusDecoder::new(sr, ch).context("Opus decoder init")?;
                log(&state, "[audio] Opus decoder ready.");
                Some(dec)
            }
            _ => {
                log(&state, "[audio] Unsupported sr/ch for Opus — falling back to PCM");
                None
            }
        }
    } else {
        None
    };

    let max_pcm_samples = (header.sample_rate as usize * 120 / 1000) * header.channels as usize;
    let mut opus_pcm_buf: Vec<i16> = vec![0; max_pcm_samples];
    let mut opus_frame_buf: Vec<u8> = vec![0; MAX_OPUS_FRAME_BYTES];

    let mut wav: Option<WavWriter> = match initial_wav_path.as_deref() {
        Some(p) => {
            log(&state, format!("[audio] Recording to {p}"));
            Some(WavWriter::create(p, header.sample_rate, header.channels, 16)?)
        }
        None => None,
    };

    let shm: Option<SharedMemWriter> = if use_driver {
        match SharedMemWriter::new(header.sample_rate, header.channels, 16) {
            Ok(s) => { log(&state, "[audio] SHM opened."); Some(s) }
            Err(e) => { log(&state, format!("[audio] SHM error: {e}")); None }
        }
    } else { None };

    let mut read_buf = vec![0u8; 4096];
    let mut proc_buf: Vec<u8> = Vec::with_capacity(4096);
    let start = Instant::now();
    let mut total_bytes: u64 = 0;
    let mut last_stats = Instant::now();
    let mut gain = initial_gain;
    let mut gate_threshold = initial_gate;
    let mut gate_hold_until = Instant::now();
    let mut gate_open = true;
    let mut lpf = LowpassFilter::new(initial_lowpass, header.sample_rate, header.channels);

    log(&state, format!(
        "[audio] Streaming... (gate={:.3} lpf={:.0}Hz)",
        gate_threshold, initial_lowpass
    ));

    'read_loop: loop {
        // Drain commands
        loop {
            match cmd_rx.try_recv() {
                Ok(Command::Stop) => { log(&state, "[audio] Stop."); break 'read_loop; }
                Ok(Command::SetGain(g)) => { gain = g; }
                Ok(Command::SetNoiseGate(t)) => {
                    gate_threshold = t;
                    log(&state, format!("[audio] Gate: {:.3}", t));
                }
                Ok(Command::SetLowpass(hz)) => {
                    lpf.update_cutoff(hz, header.sample_rate);
                    log(&state, format!("[audio] LPF: {:.0}Hz", hz));
                }
                Ok(Command::StartWav(path)) => {
                    if wav.is_none() {
                        wav = WavWriter::create(&path, header.sample_rate, header.channels, 16).ok();
                        log(&state, format!("[audio] WAV: {path}"));
                    }
                }
                Ok(Command::StopWav) => {
                    if let Some(w) = wav.take() {
                        let _ = w.finalize();
                        log(&state, "[audio] WAV stopped.");
                    }
                }
                Ok(Command::Start { .. }) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break 'read_loop,
            }
        }

        // ── Read chunk ──────────────────────────────────────────────────────
        let raw_bytes_consumed: usize;

        if header.format == FMT_OPUS && opus_decoder.is_some() {
            // Opus framing: [u16 LE frame_len][frame_len bytes]
            let mut len_buf = [0u8; 2];
            match source.read(&mut len_buf, 100) {
                Ok(0) => continue,
                Ok(1) => { read_exact(&source, &mut len_buf[1..])?; }
                Ok(_) => {}
                Err(e) => { log(&state, format!("[audio] Read err: {e}")); break; }
            }

            let frame_len = u16::from_le_bytes(len_buf) as usize;
            if frame_len == 0 || frame_len > MAX_OPUS_FRAME_BYTES {
                log(&state, format!("[audio] Bad Opus frame len: {}", frame_len));
                break;
            }

            read_exact(&source, &mut opus_frame_buf[..frame_len])?;
            raw_bytes_consumed = 2 + frame_len;

            let dec = opus_decoder.as_mut().unwrap();
            let n_samples = dec.decode(
                Some(&opus_frame_buf[..frame_len]),
                &mut opus_pcm_buf,
                false,
            ).context("Opus decode")?;

            proc_buf.clear();
            let pcm_slice = &opus_pcm_buf[..n_samples * header.channels as usize];
            proc_buf.reserve(pcm_slice.len() * 2);
            for &s in pcm_slice {
                let b = s.to_le_bytes();
                proc_buf.push(b[0]);
                proc_buf.push(b[1]);
            }
        } else {
            // Raw PCM
            let n = match source.read(&mut read_buf, 100) {
                Ok(0) => continue,
                Ok(n) => n,
                Err(e) => { log(&state, format!("[audio] Read err: {e}")); break; }
            };
            raw_bytes_consumed = n;
            proc_buf.clear();
            proc_buf.extend_from_slice(&read_buf[..n]);
        }

        // ── DSP chain ───────────────────────────────────────────────────────

        // 1. Gain
        if (gain - 1.0).abs() > 0.001 {
            for chunk in proc_buf.chunks_exact_mut(2) {
                let s = i16::from_le_bytes([chunk[0], chunk[1]]);
                let amp = (s as f32 * gain).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
                let b = amp.to_le_bytes();
                chunk[0] = b[0]; chunk[1] = b[1];
            }
        }

        // 2. Lowpass
        lpf.process(&mut proc_buf);

        // 3. RMS + noise gate
        let rms = compute_rms(&proc_buf);
        let gated_out = if gate_threshold > 0.0 {
            if rms >= gate_threshold {
                gate_hold_until =
                    Instant::now() + std::time::Duration::from_millis(GATE_HOLD_MS);
                gate_open = true;
            } else if Instant::now() >= gate_hold_until {
                gate_open = false;
            }
            !gate_open
        } else {
            gate_open = true;
            false
        };

        let chunk: Cow<[u8]> = if gated_out {
            Cow::Owned(vec![0u8; proc_buf.len()])
        } else {
            Cow::Borrowed(&proc_buf)
        };

        total_bytes += raw_bytes_consumed as u64;

        if let Some(ref s) = shm { s.write(&chunk); }
        if let Some(ref mut w) = wav { let _ = w.append(&chunk); }

        // Stats update ~100ms
        if last_stats.elapsed().as_millis() >= 100 {
            last_stats = Instant::now();
            let (wi, ri) = if let Some(ref s) = shm { s.indices() } else { (0, 0) };
            if let Ok(mut st) = state.lock() {
                st.stats.bytes_received = total_bytes;
                st.stats.elapsed_secs = start.elapsed().as_secs_f64();
                st.stats.rms = rms;
                st.stats.gate_active = gated_out;
                st.stats.shm_write_idx = wi;
                st.stats.shm_read_idx = ri;
                if st.stats.rms_history.len() >= GRAPH_HISTORY {
                    st.stats.rms_history.pop_front();
                }
                st.stats.rms_history.push_back(rms);
            }
        }
    }

    if let Some(w) = wav { let _ = w.finalize(); log(&state, "[audio] WAV finalized."); }
    if let Ok(mut st) = state.lock() {
        st.status = ConnectionStatus::Disconnected;
        st.push_log("[audio] Session ended.".to_string());
    }
    Ok(())
}
