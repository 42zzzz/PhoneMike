use anyhow::Result;
use byteorder::{LittleEndian, WriteBytesExt};
use std::io::{Seek, SeekFrom, Write};
use std::fs::File;

/// Writes a WAV file incrementally. Call `append()` for each PCM chunk,
/// `finalize()` at end to patch the header with correct byte counts.
pub struct WavWriter {
    file: File,
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    data_bytes: u32,
}

impl WavWriter {
    pub fn create(path: &str, sample_rate: u32, channels: u16, bits_per_sample: u16) -> Result<Self> {
        let mut file = File::create(path)?;
        // Write placeholder header (44 bytes) — will be patched in finalize()
        write_wav_header(&mut file, sample_rate, channels, bits_per_sample, 0)?;
        Ok(WavWriter { file, sample_rate, channels, bits_per_sample, data_bytes: 0 })
    }

    pub fn append(&mut self, pcm: &[u8]) -> Result<()> {
        self.file.write_all(pcm)?;
        self.data_bytes += pcm.len() as u32;
        Ok(())
    }

    pub fn finalize(mut self) -> Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        write_wav_header(
            &mut self.file,
            self.sample_rate,
            self.channels,
            self.bits_per_sample,
            self.data_bytes,
        )?;
        Ok(())
    }
}

fn write_wav_header(
    w: &mut impl Write,
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    data_bytes: u32,
) -> Result<()> {
    let byte_rate = sample_rate * channels as u32 * bits_per_sample as u32 / 8;
    let block_align = channels * bits_per_sample / 8;

    // RIFF chunk
    w.write_all(b"RIFF")?;
    w.write_u32::<LittleEndian>(36 + data_bytes)?; // file size - 8
    w.write_all(b"WAVE")?;

    // fmt sub-chunk
    w.write_all(b"fmt ")?;
    w.write_u32::<LittleEndian>(16)?;               // sub-chunk size
    w.write_u16::<LittleEndian>(1)?;                // PCM = 1
    w.write_u16::<LittleEndian>(channels)?;
    w.write_u32::<LittleEndian>(sample_rate)?;
    w.write_u32::<LittleEndian>(byte_rate)?;
    w.write_u16::<LittleEndian>(block_align)?;
    w.write_u16::<LittleEndian>(bits_per_sample)?;

    // data sub-chunk
    w.write_all(b"data")?;
    w.write_u32::<LittleEndian>(data_bytes)?;

    Ok(())
}
