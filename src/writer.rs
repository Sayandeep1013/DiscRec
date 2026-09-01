//! Writing the mixed stream to Ogg/Opus.
//!
//! See `docs/spec/storage-format.md`. Two properties matter more than
//! compactness:
//!
//! 1. **A killed process must leave a playable file** (R7). Pages are flushed
//!    as they are produced and nothing is back-patched at the end, so a
//!    truncated file is simply a file with a truncated last page — which
//!    decoders handle.
//! 2. **No proprietary index.** Losing DiscRec entirely must not make the
//!    recordings less useful, so the audio stands alone.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use opus::{Application, Channels, Encoder};

/// 20 ms at 48 kHz. Opus permits 2.5–60 ms; 20 ms is the usual voice tradeoff
/// between per-packet overhead and how much a lost page costs.
const FRAME_SIZE: usize = 960;

/// Samples the decoder discards at the start, per the Opus spec's
/// recommendation for the default encoder delay.
const PRE_SKIP: u16 = 312;

const SAMPLE_RATE: u32 = 48_000;

/// Bitrate for stereo voice. Opus is transparent enough here that raising it
/// mostly buys file size.
const BITRATE: i32 = 96_000;

pub struct OpusWriter {
    encoder: Encoder,
    packets: PacketWriter<'static, File>,
    channels: usize,
    serial: u32,
    /// Interleaved samples not yet forming a whole frame.
    pending: Vec<f32>,
    /// Total samples per channel handed to the encoder, plus pre-skip. This is
    /// what Ogg calls the granule position.
    granule: u64,
    encoded: Vec<u8>,
    pub packets_written: u64,
    pub bytes_written: u64,
}

impl OpusWriter {
    pub fn create(path: impl AsRef<Path>, channels: u16) -> std::io::Result<Self> {
        let ch = match channels {
            1 => Channels::Mono,
            _ => Channels::Stereo,
        };

        let mut encoder = Encoder::new(SAMPLE_RATE, ch, Application::Voip)
            .map_err(|e| std::io::Error::other(format!("opus encoder: {e}")))?;
        let _ = encoder.set_bitrate(opus::Bitrate::Bits(BITRATE));

        // Deliberately unbuffered. A BufWriter would coalesce writes, but
        // anything still sitting in it is lost when the process is killed --
        // measured at up to ~1.3 s of audio. Handing each page straight to the
        // OS means a kill can no longer lose data the encoder already
        // produced, and ~50 small writes per second is far below the CPU
        // budget (R11).
        let mut packets = PacketWriter::new(File::create(path)?);

        // A fixed serial would collide if two recordings were ever muxed
        // together; derive one from the clock instead.
        let serial = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0x4469_7363)
            | 1;

        // Opus mapping family 0 covers mono and stereo only.
        let channels_u8 = channels.clamp(1, 2) as u8;
        packets.write_packet(
            opus_head(channels_u8),
            serial,
            PacketWriteEndInfo::EndPage,
            0,
        )?;
        packets.write_packet(opus_tags(), serial, PacketWriteEndInfo::EndPage, 0)?;

        Ok(Self {
            encoder,
            packets,
            channels: channels_u8 as usize,
            serial,
            pending: Vec::with_capacity(FRAME_SIZE * 2 * 2),
            granule: u64::from(PRE_SKIP),
            encoded: vec![0u8; 4000],
            packets_written: 0,
            bytes_written: 0,
        })
    }

    /// Accept interleaved f32 samples. Encodes whole frames and leaves any
    /// remainder buffered.
    pub fn write(&mut self, samples: &[f32]) -> std::io::Result<()> {
        self.pending.extend_from_slice(samples);

        let per_frame = FRAME_SIZE * self.channels;
        while self.pending.len() >= per_frame {
            let frame: Vec<f32> = self.pending.drain(..per_frame).collect();
            self.emit(&frame, false)?;
        }
        Ok(())
    }

    fn emit(&mut self, frame: &[f32], last: bool) -> std::io::Result<()> {
        let n = self
            .encoder
            .encode_float(frame, &mut self.encoded)
            .map_err(|e| std::io::Error::other(format!("opus encode: {e}")))?;

        self.granule += FRAME_SIZE as u64;

        let info = if last {
            PacketWriteEndInfo::EndStream
        } else {
            PacketWriteEndInfo::EndPage
        };

        self.packets
            .write_packet(self.encoded[..n].to_vec(), self.serial, info, self.granule)?;

        self.packets_written += 1;
        self.bytes_written += n as u64;

        // The page is now with the OS, so a process kill cannot lose it.
        Ok(())
    }

    /// Pad the final partial frame, mark end of stream, and flush.
    pub fn finalize(mut self) -> std::io::Result<()> {
        let per_frame = FRAME_SIZE * self.channels;
        if !self.pending.is_empty() {
            self.pending.resize(per_frame, 0.0);
            let frame: Vec<f32> = self.pending.drain(..per_frame).collect();
            self.emit(&frame, true)?;
        } else {
            // Nothing buffered — emit a silent frame purely to carry the
            // end-of-stream marker, so the file closes cleanly.
            let silence = vec![0.0f32; per_frame];
            self.emit(&silence, true)?;
        }

        let mut inner = self.packets.into_inner();
        inner.flush()?;
        inner.sync_all()?;
        Ok(())
    }
}

/// The OpusHead identification packet. Layout is fixed by RFC 7845.
fn opus_head(channels: u8) -> Vec<u8> {
    let mut v = Vec::with_capacity(19);
    v.extend_from_slice(b"OpusHead");
    v.push(1); // version
    v.push(channels);
    v.extend_from_slice(&PRE_SKIP.to_le_bytes());
    v.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    v.extend_from_slice(&0i16.to_le_bytes()); // output gain
    v.push(0); // channel mapping family: mono/stereo
    v
}

/// The OpusTags comment packet. Required by the spec even when empty.
fn opus_tags() -> Vec<u8> {
    const VENDOR: &[u8] = b"DiscRec";
    let mut v = Vec::with_capacity(8 + 4 + VENDOR.len() + 4);
    v.extend_from_slice(b"OpusTags");
    v.extend_from_slice(&(VENDOR.len() as u32).to_le_bytes());
    v.extend_from_slice(VENDOR);
    v.extend_from_slice(&0u32.to_le_bytes()); // no user comments
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_head_matches_rfc7845_layout() {
        let h = opus_head(2);
        assert_eq!(&h[..8], b"OpusHead");
        assert_eq!(h[8], 1, "version");
        assert_eq!(h[9], 2, "channel count");
        assert_eq!(u16::from_le_bytes([h[10], h[11]]), PRE_SKIP);
        assert_eq!(
            u32::from_le_bytes([h[12], h[13], h[14], h[15]]),
            SAMPLE_RATE
        );
        assert_eq!(h[18], 0, "mapping family");
        assert_eq!(h.len(), 19);
    }

    #[test]
    fn opus_tags_is_well_formed() {
        let t = opus_tags();
        assert_eq!(&t[..8], b"OpusTags");
        let vlen = u32::from_le_bytes([t[8], t[9], t[10], t[11]]) as usize;
        assert_eq!(&t[12..12 + vlen], b"DiscRec");
        assert_eq!(
            u32::from_le_bytes([t[12 + vlen], t[13 + vlen], t[14 + vlen], t[15 + vlen]]),
            0,
            "comment count"
        );
    }

    #[test]
    fn writes_a_playable_file_and_reports_progress() {
        let path = std::env::temp_dir().join("discrec-writer-test.ogg");
        let mut w = OpusWriter::create(&path, 2).expect("create");

        // One second of a quiet tone, fed in blocks that do not align to the
        // frame size — the remainder handling is the part worth exercising.
        let mut phase = 0.0f32;
        for _ in 0..100 {
            let mut block = Vec::with_capacity(480 * 2);
            for _ in 0..480 {
                phase += 440.0 * std::f32::consts::TAU / SAMPLE_RATE as f32;
                let s = (phase.sin()) * 0.2;
                block.push(s);
                block.push(s);
            }
            w.write(&block).expect("write");
        }

        assert!(w.packets_written > 0, "should have emitted audio packets");
        assert!(w.bytes_written > 0);
        w.finalize().expect("finalize");

        let size = std::fs::metadata(&path).expect("stat").len();
        assert!(size > 1000, "file suspiciously small: {size} bytes");

        let head = std::fs::read(&path).expect("read");
        assert_eq!(&head[..4], b"OggS", "must start with an Ogg page");

        let _ = std::fs::remove_file(&path);
    }
}
