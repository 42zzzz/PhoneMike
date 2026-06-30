use std::collections::VecDeque;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use audiopus::coder::Decoder as OpusDecoder;
use audiopus::{Channels, SampleRate};
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Cursor;

use crate::output;
use crate::state::{
    AppStateHandle, AudioStats, Command, ConnectionStatus, GRAPH_HISTORY,
};
use crate::tcp::TcpTransport;
use crate::wav::WavWriter;

const HEADER_SIZE: usize = 16;
const MAGIC: &[u8; 4] = b"PHMC";

const FMT_OPUS: u16 = 2;

const GATE_HOLD_MS: u64 = 80;

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

fn compute_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples
        .iter()
        .map(|&s| (s as f64) * (s as f64))
        .sum();
    (sum / samples.len() as f64).sqrt() as f32 / i16::MAX as f32
}

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

    fn process(&mut self, buf: &mut [i16]) {
        if self.is_bypass() { return; }
        let ch = self.state.len();
        for (frame_idx, sample) in buf.iter_mut().enumerate() {
            let ch_idx = frame_idx % ch;
            let s = *sample as f32;
            let y = self.state[ch_idx] + self.alpha * (s - self.state[ch_idx]);
            self.state[ch_idx] = y;
            *sample = y.clamp(i16::MIN as f32, i16::MAX as f32) as i16;
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

pub fn run_audio_thread(cmd_rx: Receiver<Command>, state: AppStateHandle, device_name: Option<String>) {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL,
        };
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
    }

    loop {
        let cmd = match cmd_rx.recv() {
            Ok(c) => c,
            Err(_) => return,
        };

        let (initial_wav_path, initial_gain, initial_gate, initial_lowpass) = match cmd {
            Command::Start { wav_path, gain, noise_gate, lowpass_hz } => {
                (wav_path, gain, noise_gate, lowpass_hz)
            }
            _ => continue,
        };

        let dev_name = device_name.clone();

        loop {
            match stream_session(
                &cmd_rx,
                Arc::clone(&state),
                dev_name.as_deref(),
                initial_wav_path.clone(),
                initial_gain,
                initial_gate,
                initial_lowpass,
            ) {
                Ok(stopped) => {
                    if stopped {
                        break;
                    }
                    log(&state, "[audio] Disconnected. Reconnecting...");
                }
                Err(e) => {
                    let msg = format!("Session error: {e:#}");
                    log(&state, &msg);
                }
            }

            match cmd_rx.try_recv() {
                Ok(Command::Stop) => {
                    if let Ok(mut st) = state.lock() {
                        st.status = ConnectionStatus::Disconnected;
                    }
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                _ => {}
            }

            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
}

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
    device_name: Option<&str>,
    initial_wav_path: Option<String>,
    initial_gain: f32,
    initial_gate: f32,
    initial_lowpass: f32,
) -> Result<bool> {
    {
        let mut st = state.lock().unwrap();
        st.status = ConnectionStatus::Connecting;
        st.push_log("Connecting via ADB/TCP...".to_string());
    }

    match TcpTransport::setup_forward(18501) {
        Ok(_) => log(&state, "[audio] ADB forward OK. Waiting for phone app..."),
        Err(e) => log(&state, format!("[audio] ADB forward failed ({e:#}). Will retry...")),
    }

    let mut attempt = 0u32;
    let source = loop {
        match cmd_rx.try_recv() {
            Ok(Command::Stop) => {
                log(&state, "[audio] Cancelled.");
                if let Ok(mut st) = state.lock() {
                    st.status = ConnectionStatus::Disconnected;
                }
                return Ok(true);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return Ok(true),
            _ => {}
        }

        if attempt > 0 && attempt % 20 == 0 {
            if let Err(e) = TcpTransport::setup_forward(18501) {
                log(&state, format!("[audio] ADB re-forward failed: {e:#}"));
            }
        }

        match TcpTransport::try_connect(18501) {
            Ok(Some(t)) => break t,
            Ok(None) => {
                attempt += 1;
                if attempt == 1 || attempt % 10 == 0 {
                    log(&state, format!(
                        "[audio] Waiting for phone... (attempt {})", attempt
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            Err(e) => {
                attempt += 1;
                if attempt % 10 == 0 {
                    log(&state, format!("[audio] Connect error: {e:#}"));
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
    }

    // ── Audio output setup ──────────────────────────────────────────────
    let output_device = match device_name {
        Some(name) => {
            log(&state, format!("[audio] Using specified device: {name}"));
            name.to_string()
        }
        None => {
            match output::find_virtual_cable() {
                Some(name) => {
                    log(&state, format!("[audio] Detected virtual cable: {name}"));
                    name
                }
                None => {
                    log(&state, "[audio] WARNING: No virtual audio cable found. Phone audio will not be heard on any device.");
                    log(&state, "[audio] Install VB-Cable (free) from https://vb-audio.com/Cable/");
                    log(&state, "[audio] Then restart PhoneMike. In your target app, select 'CABLE Output' as mic.");
                    return Err(anyhow::anyhow!(
                        "No virtual audio cable detected. Install VB-Cable (free) from vb-audio.com"
                    ));
                }
            }
        }
    };

    let audio_buffer: Arc<Mutex<VecDeque<i16>>> = Arc::new(Mutex::new(VecDeque::new()));
    let _stream = output::start_playback(
        &output_device,
        header.sample_rate,
        header.channels,
        Arc::clone(&audio_buffer),
    ).map_err(|e| {
        log(&state, format!("[audio] Failed to start audio output: {e}"));
        e
    })?;
    log(&state, "[audio] Audio output started.");

    // Build Opus decoder
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

    let mut read_buf = vec![0u8; 16384];
    let mut proc_buf: Vec<i16> = Vec::with_capacity(8192);
    let mut silence_buf: Vec<i16> = Vec::with_capacity(8192);
    let start = Instant::now();
    let mut total_bytes: u64 = 0;
    let mut last_stats = Instant::now();
    let mut gain = initial_gain;
    let mut gate_threshold = initial_gate;
    let mut gate_hold_until = Instant::now();
    let mut gate_open = true;
    let mut lpf = LowpassFilter::new(initial_lowpass, header.sample_rate, header.channels);
    let mut user_stopped = false;

    log(&state, format!(
        "[audio] Streaming... (gate={:.3} lpf={:.0}Hz)",
        gate_threshold, initial_lowpass
    ));

    'read_loop: loop {
        // Drain commands
        loop {
            match cmd_rx.try_recv() {
                Ok(Command::Stop) => { log(&state, "[audio] Stop."); user_stopped = true; break 'read_loop; }
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
            let mut len_buf = [0u8; 2];
            if read_exact(&source, &mut len_buf).is_err() {
                log(&state, "[audio] Failed to read Opus frame length");
                break;
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
            proc_buf.extend_from_slice(pcm_slice);
        } else {
            let n = match source.read(&mut read_buf, 100) {
                Ok(0) => continue,
                Ok(n) => n,
                Err(e) => { log(&state, format!("[audio] Read err: {e}")); break; }
            };
            raw_bytes_consumed = n;
            proc_buf.clear();
            let i16_count = n / 2;
            let i16_slice = unsafe {
                std::slice::from_raw_parts(read_buf.as_ptr() as *const i16, i16_count)
            };
            proc_buf.extend_from_slice(i16_slice);
        }

        // ── DSP chain ───────────────────────────────────────────────────────
        let now = Instant::now();

        if (gain - 1.0).abs() > 0.001 {
            for s in proc_buf.iter_mut() {
                *s = ((*s as f32 * gain).clamp(i16::MIN as f32, i16::MAX as f32)) as i16;
            }
        }

        lpf.process(&mut proc_buf);

        let rms = compute_rms(&proc_buf);
        let gated_out = if gate_threshold > 0.0 {
            if rms >= gate_threshold {
                gate_hold_until = now + std::time::Duration::from_millis(GATE_HOLD_MS);
                gate_open = true;
            } else if now >= gate_hold_until {
                gate_open = false;
            }
            !gate_open
        } else {
            gate_open = true;
            false
        };

        let chunk: &[i16] = if gated_out {
            if silence_buf.len() < proc_buf.len() {
                silence_buf.resize(proc_buf.len(), 0i16);
            }
            &silence_buf[..proc_buf.len()]
        } else {
            &proc_buf
        };

        total_bytes += raw_bytes_consumed as u64;

        // Push PCM to audio output buffer
        if !chunk.is_empty() {
            let mut buf = audio_buffer.lock().unwrap();
            buf.extend(chunk);
            // Maintain buffer at ~40ms to absorb CPU load spikes without adding latency
            let target = (header.sample_rate as usize * header.channels as usize) / 25;
            let max = target * 2;
            let current_len = buf.len();
            if current_len > max {
                buf.drain(0..current_len - target);
            }
        }

        if let Some(ref mut w) = wav {
            let byte_slice = unsafe {
                std::slice::from_raw_parts(chunk.as_ptr() as *const u8, chunk.len() * 2)
            };
            let _ = w.append(byte_slice);
        }

        if now.duration_since(last_stats).as_millis() >= 100 {
            last_stats = now;
            if let Ok(mut st) = state.lock() {
                st.stats.bytes_received = total_bytes;
                st.stats.elapsed_secs = start.elapsed().as_secs_f64();
                st.stats.rms = rms;
                st.stats.gate_active = gated_out;
                if st.stats.rms_history.len() >= GRAPH_HISTORY {
                    st.stats.rms_history.pop_front();
                }
                st.stats.rms_history.push_back(rms);
            }
        }
    }

    if let Some(w) = wav { let _ = w.finalize(); log(&state, "[audio] WAV finalized."); }
    if let Ok(mut st) = state.lock() {
        if user_stopped {
            st.status = ConnectionStatus::Disconnected;
        }
        st.push_log("[audio] Session ended.".to_string());
    }
    Ok(user_stopped)
}
