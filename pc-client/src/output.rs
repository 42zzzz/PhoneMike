use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Find a virtual audio cable output device (VB-Cable, VoiceMeeter, etc.).
/// Returns the device name, or None if not found.
/// Prefers VB-Cable ("CABLE Input") over VoiceMeeter.
pub fn find_virtual_cable() -> Option<String> {
    let host = cpal::default_host();
    let Ok(devices) = host.output_devices() else { return None };
    let mut voicemeeter = None;
    for device in devices {
        let Ok(name) = device.name() else { continue };
        let lower = name.to_lowercase();
        // VB-Cable is the preferred target — return immediately.
        if lower.contains("cable input") || lower.contains("vb-cable") {
            return Some(name);
        }
        // Remember VoiceMeeter as a fallback.
        if lower.contains("voicemeeter input") || lower.contains("voice meeter input") {
            voicemeeter = Some(name);
        }
    }
    voicemeeter
}

/// Print all available audio output devices to stderr (debug helper).
pub fn list_devices() {
    let host = cpal::default_host();
    let Ok(devices) = host.output_devices() else {
        eprintln!("[output] No output devices found.");
        return;
    };
    eprintln!("[output] Available audio output devices:");
    for device in devices {
        if let Ok(name) = device.name() {
            eprintln!("[output]   {name}");
        }
    }
}

/// Start WASAPI playback on the given output device.
/// `sample_rate` and `channels` come from the phone stream (typically 48000, 1).
/// `buffer` is the shared i16 sample buffer the callback reads from.
/// Keep the returned `cpal::Stream` alive to keep audio playing.
pub fn start_playback(
    device_name: &str,
    sample_rate: u32,
    channels: u16,
    buffer: Arc<Mutex<VecDeque<i16>>>,
) -> Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = host
        .output_devices()
        .context("no output devices available")?
        .find(|d| d.name().map(|n| n == device_name).unwrap_or(false))
        .with_context(|| format!("audio device \"{device_name}\" not found"))?;

    let default_cfg = device
        .default_output_config()
        .context("no default output config")?;

    // Request the phone's native format (e.g. 48000 Hz, mono).
    // WASAPI shared mode resamples to the device's mix format as needed.
    let phone_config_small = cpal::StreamConfig {
        channels: channels as u16,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Fixed(480),
    };

    let phone_config_default = cpal::StreamConfig {
        channels: channels as u16,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let out_channels = phone_config_default.channels as usize;
    let sample_format = default_cfg.sample_format();

    // Try small buffer (10ms) first, then default phone format, then device default.
    let stream_result = 'build: {
        let r = try_build(&device, &phone_config_small, sample_format, Arc::clone(&buffer), out_channels);
        if let Ok(stream) = r {
            eprintln!("[output] Using 10ms WASAPI buffer");
            break 'build Ok(stream);
        }
        let r = try_build(&device, &phone_config_default, sample_format, Arc::clone(&buffer), out_channels);
        if let Ok(stream) = r {
            break 'build Ok(stream);
        }
        let fallback_config = default_cfg.config();
        let fo = fallback_config.channels as usize;
        eprintln!(
            "[output] Phone format ({}Hz/{}ch) not supported, \
             falling back to device default ({}Hz/{}ch)",
            sample_rate, channels,
            fallback_config.sample_rate.0, fallback_config.channels,
        );
        try_build(&device, &fallback_config, sample_format, buffer, fo)
    };

    let stream = stream_result.map_err(|e| anyhow::anyhow!("build_output_stream: {e}"))?;
    stream.play().context("stream.play")?;
    Ok(stream)
}

fn try_build(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    buffer: Arc<Mutex<VecDeque<i16>>>,
    out_channels: usize,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let err_fn = |err: cpal::StreamError| {
        eprintln!("[output] Playback error: {err}");
    };
    let buf = Arc::clone(&buffer);
    match sample_format {
        cpal::SampleFormat::F32 => device.build_output_stream::<f32, _, _>(
            config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut src = buf.lock().unwrap();
                for frame in data.chunks_exact_mut(out_channels) {
                    let s = src.pop_front().unwrap_or(0) as f32 / i16::MAX as f32;
                    for ch in frame.iter_mut() { *ch = s; }
                }
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_output_stream::<i16, _, _>(
            config,
            move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                let mut src = buf.lock().unwrap();
                for frame in data.chunks_exact_mut(out_channels) {
                    let s = src.pop_front().unwrap_or(0);
                    for ch in frame.iter_mut() { *ch = s; }
                }
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_output_stream::<u16, _, _>(
            config,
            move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                let mut src = buf.lock().unwrap();
                for frame in data.chunks_exact_mut(out_channels) {
                    let s = src.pop_front().unwrap_or(0) as i32 + 32768;
                    for ch in frame.iter_mut() { *ch = s as u16; }
                }
            },
            err_fn,
            None,
        ),
        _ => Err(cpal::BuildStreamError::StreamConfigNotSupported),
    }
}
