//! Audio decoding for the transcription endpoint.
//!
//! DeepSeek's speech-to-text API takes 16-bit PCM WAV or Ogg Opus only, while
//! the clients that talk to this service record whatever their platform gives
//! them: a browser records WebM/Opus, an iPhone records MP4/AAC, a voice memo
//! arrives as MP3 or FLAC. Rather than pushing that mismatch onto every
//! downstream, this module decodes any container DeepSeek cannot read and
//! re-encodes it as the one shape DeepSeek always accepts: mono 16 kHz 16-bit
//! PCM WAV, which is also the smallest form of the accepted set.
//!
//! Everything here is pure Rust (`symphonia` for the containers and codecs,
//! `opus-decoder` for Opus, which symphonia does not cover), so the service
//! stays a single binary with no ffmpeg on the host.

use std::io::Cursor;

use opus_decoder::OpusDecoder;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_OPUS, DecoderOptions};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::AppError;

/// The sample rate every transcoded upload is written at.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Opus always decodes at 48 kHz, whatever rate its header advertises.
const OPUS_SAMPLE_RATE: u32 = 48_000;

/// Longest Opus frame Opus allows, per channel: 120 ms at 48 kHz.
const OPUS_MAX_FRAME: usize = 5_760;

/// Taps on each side of a resampled sample.
const RESAMPLE_HALF_TAPS: isize = 16;

/// One upload, decoded and re-encoded as a WAV DeepSeek can read.
#[derive(Debug)]
pub struct Transcode {
    /// The complete WAV file: header and samples.
    pub wav: Vec<u8>,
    /// Duration of the decoded audio, in milliseconds.
    pub duration_ms: i64,
    /// The rate the upload was decoded at, before resampling to 16 kHz.
    pub source_rate: u32,
    /// How many channels the upload carried, before downmixing to mono.
    pub source_channels: usize,
}

/// Decodes any audio container this build knows and returns a mono 16 kHz
/// 16-bit PCM WAV. A body that is not audio at all, or whose codec is not
/// built in, is answered with the formats DeepSeek does take.
pub fn to_wav(bytes: &[u8]) -> Result<Transcode, AppError> {
    let source = MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), Default::default());
    let probed = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|_| unreadable())?;
    let mut format = probed.format;
    let track = format.default_track().ok_or_else(unreadable)?;
    let track_id = track.id;
    let params = track.codec_params.clone();
    let source_channels = params
        .channels
        .map(|channels| channels.count())
        .unwrap_or(1)
        .max(1);
    let is_opus = params.codec == CODEC_TYPE_OPUS;
    let source_rate = if is_opus {
        OPUS_SAMPLE_RATE
    } else {
        params.sample_rate.unwrap_or(TARGET_SAMPLE_RATE)
    };
    let mut decoder = if is_opus {
        None
    } else {
        Some(
            symphonia::default::get_codecs()
                .make(&params, &DecoderOptions::default())
                .map_err(|_| unreadable())?,
        )
    };
    let mut opus = if is_opus {
        Some(OpusDecoder::new(OPUS_SAMPLE_RATE, source_channels).map_err(|_| unreadable())?)
    } else {
        None
    };
    let mut mono: Vec<f32> = Vec::new();
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        if let Some(opus) = opus.as_mut() {
            let mut pcm = vec![0i16; OPUS_MAX_FRAME * source_channels];
            let frames = match opus.decode(&packet.data, &mut pcm, false) {
                Ok(frames) => frames,
                // A damaged packet costs its own frame, not the upload.
                Err(_) => continue,
            };
            for frame in pcm[..frames * source_channels].chunks(source_channels) {
                mono.push(mean(frame));
            }
        } else if let Some(decoder) = decoder.as_mut() {
            let decoded = match decoder.decode(&packet) {
                Ok(decoded) => decoded,
                Err(_) => continue,
            };
            let frames = decoded.frames();
            let mut buffer = SampleBuffer::<f32>::new(frames as u64, *decoded.spec());
            buffer.copy_interleaved_ref(decoded);
            for frame in buffer.samples().chunks(source_channels) {
                mono.push(mean_i16_or_f32(frame));
            }
        }
    }
    if mono.is_empty() {
        return Err(unreadable());
    }
    let (samples, rate) = if source_rate == 8_000 || source_rate == TARGET_SAMPLE_RATE {
        (mono, source_rate)
    } else {
        (
            resample_mono(&mono, source_rate, TARGET_SAMPLE_RATE),
            TARGET_SAMPLE_RATE,
        )
    };
    Ok(Transcode {
        duration_ms: samples.len() as i64 * 1_000 / i64::from(rate),
        wav: wav_bytes(&samples, rate),
        source_rate,
        source_channels,
    })
}

fn unreadable() -> AppError {
    AppError::bad_request(
        "the audio could not be decoded; DeepSeek transcribes 16-bit PCM WAV or Ogg Opus, \
         and this service also converts WebM/Opus, MP4/AAC, MP3, FLAC and Vorbis uploads",
    )
}

fn mean(samples: &[i16]) -> f32 {
    let sum: i32 = samples.iter().map(|sample| i32::from(*sample)).sum();
    sum as f32 / (samples.len().max(1) as f32 * 32_768.0)
}

fn mean_i16_or_f32(samples: &[f32]) -> f32 {
    let sum: f32 = samples.iter().sum();
    sum / (samples.len().max(1) as f32)
}

/// Resamples mono f32 audio with a windowed-sinc kernel.
///
/// The kernel is evaluated per output sample rather than precomputed per phase,
/// which keeps the integer ratios (48 kHz → 16 kHz) and the awkward ones
/// (44.1 kHz → 16 kHz) on the same code path. Gains are normalised by the
/// window sum, so a constant signal stays constant.
fn resample_mono(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let step = f64::from(from) / f64::from(to);
    let output_len = (input.len() as f64 / step).round() as usize;
    let cutoff = 0.5 * (f64::from(to) / f64::from(from)).min(1.0);
    let mut output = Vec::with_capacity(output_len);
    for index in 0..output_len {
        let center = index as f64 * step;
        let base = center.floor() as isize;
        let (mut sum, mut weight) = (0.0f64, 0.0f64);
        for tap in -RESAMPLE_HALF_TAPS..=RESAMPLE_HALF_TAPS {
            let position = base + tap;
            if position < 0 || position as usize >= input.len() {
                continue;
            }
            let distance = center - position as f64;
            let window =
                0.5 + 0.5 * (std::f64::consts::PI * distance / RESAMPLE_HALF_TAPS as f64).cos();
            let sinc = if distance.abs() < 1e-9 {
                2.0 * cutoff
            } else {
                (2.0 * std::f64::consts::PI * cutoff * distance).sin()
                    / (std::f64::consts::PI * distance)
            };
            let kernel = sinc * window;
            sum += f64::from(input[position as usize]) * kernel;
            weight += kernel;
        }
        output.push(if weight.abs() > 1e-9 {
            (sum / weight) as f32
        } else {
            0.0
        });
    }
    output
}

/// One mono 16-bit PCM WAV file.
fn wav_bytes(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * 32_767.0).round() as i16;
        wav.extend_from_slice(&value.to_le_bytes());
    }
    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_constant_signal_survives_resampling() {
        let input = vec![0.5f32; 48_000];
        let output = resample_mono(&input, 48_000, 16_000);
        assert_eq!(output.len(), 16_000);
        assert!(output.iter().all(|sample| (sample - 0.5).abs() < 1e-3));
    }

    #[test]
    fn integer_ratios_keep_the_duration_and_awkward_ones_stay_close() {
        let forty_eight = resample_mono(&vec![0.0f32; 48_000], 48_000, 16_000);
        assert_eq!(forty_eight.len(), 16_000);
        let forty_four_one = resample_mono(&vec![0.0f32; 44_100], 44_100, 16_000);
        assert!((forty_four_one.len() as i64 - 16_000).abs() <= 1);
    }

    #[test]
    fn the_written_wav_is_sixteen_bit_mono_pcm() {
        let wav = wav_bytes(&[0.0, 0.5, -0.5], 16_000);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u16::from_le_bytes([wav[20], wav[21]]), 1, "mono");
        assert_eq!(
            u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
            16_000
        );
        assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 16, "16-bit");
        assert_eq!(u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]), 6);
        assert_eq!(wav.len(), 44 + 6);
    }

    #[test]
    fn a_wav_upload_is_decoded_end_to_end() {
        // 0.25 s of 44.1 kHz mono silence, written by hand.
        let samples = vec![0.25f32; 11_025];
        let wav = wav_bytes(&samples, 44_100);
        let decoded = to_wav(&wav).expect("our own WAV decodes");
        assert_eq!(decoded.source_rate, 44_100);
        assert_eq!(decoded.source_channels, 1);
        assert_eq!(decoded.duration_ms, 250);
        assert_eq!(&decoded.wav[0..4], b"RIFF");
        assert_eq!(
            u32::from_le_bytes([
                decoded.wav[24],
                decoded.wav[25],
                decoded.wav[26],
                decoded.wav[27]
            ]),
            16_000
        );
        assert_eq!(decoded.wav.len(), 44 + 16_000 / 4 * 2);
    }

    #[test]
    fn bytes_that_are_not_audio_are_refused_with_the_accepted_formats() {
        let error = to_wav(b"definitely not audio").expect_err("garbage is refused");
        assert!(error.message().contains("16-bit PCM WAV or Ogg Opus"));
        assert!(error.message().contains("WebM/Opus"));
    }
}
