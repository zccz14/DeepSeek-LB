//! DeepSeek Speech-to-Text API.
//!
//! `POST /v1/tools/asr` transcribes one complete audio file: the whole upload
//! goes in a single `multipart/form-data` request and the complete transcript
//! comes back. There is no streaming, no model choice and no language
//! parameter — DeepSeek detects the language from the audio and bills per
//! second of audio rather than per token.
//!
//! Downstream sees the same capability in OpenAI's shape
//! (`POST /v1/audio/transcriptions` with the audio in the `file` field), which
//! is what every speech-to-text client and gateway already speaks. This module
//! reads that upload, rebuilds the one field DeepSeek wants, and validates the
//! audio before it is sent, so an unsupported container, an oversized file or
//! an over-long recording is answered with our own message instead of
//! DeepSeek's terse 400.

use axum::body::Bytes;
use multer::Multipart;
use serde_json::Value;

use crate::AppError;

/// Path appended to the configured upstream base URL.
pub const ASR_PATH: &str = "/v1/tools/asr";

/// Value stored in the audit row's `model` column. DeepSeek's ASR serves one
/// fixed model and takes no `model` field, but operators filter the audit page
/// by model, so the endpoint names itself.
pub const AUDIT_MODEL: &str = "deepseek-asr";

/// Largest audio DeepSeek accepts in one request (8 MiB).
pub const MAX_AUDIO_BYTES: usize = 8 * 1024 * 1024;

/// Longest audio DeepSeek accepts in one request.
pub const MAX_AUDIO_MS: i64 = 120_000;

/// The audio containers DeepSeek accepts, detected from the bytes themselves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Container {
    /// 16-bit PCM WAV, the sample rate detected by DeepSeek from the header.
    Wav,
    /// Ogg Opus.
    OggOpus,
}

impl Container {
    /// The file name the forwarded part carries: DeepSeek sniffs the content,
    /// and a fixed name keeps a caller's name out of the upstream request.
    fn file_name(self) -> &'static str {
        match self {
            Self::Wav => "audio.wav",
            Self::OggOpus => "audio.opus",
        }
    }

    fn media_type(self) -> &'static str {
        match self {
            Self::Wav => "audio/wav",
            Self::OggOpus => "audio/ogg",
        }
    }
}

/// One audio upload read out of a downstream transcription request.
#[derive(Debug)]
pub struct AudioUpload {
    pub bytes: Bytes,
    /// The container the bytes prove, once [`validate`] accepted them.
    pub container: Container,
}

/// Reads the audio out of a `multipart/form-data` transcription body and the
/// model the caller named for the audit row. The scan is boundary-anchored, so
/// bytes inside the audio payload can never be mistaken for a field, and any
/// other parameter (`language`, `prompt`, `response_format`, …) is read past
/// rather than forwarded: DeepSeek's ASR takes the file only.
pub async fn read_upload(
    content_type: &str,
    body: Bytes,
) -> Result<(AudioUpload, Option<String>), AppError> {
    let boundary = multer::parse_boundary(content_type).map_err(|_| {
        AppError::bad_request("multipart/form-data body with a boundary is required")
    })?;
    let stream = futures_util::stream::once(async move { Ok::<_, std::io::Error>(body) });
    let mut multipart = Multipart::new(stream, boundary);
    let mut audio: Option<Bytes> = None;
    let mut model: Option<String> = None;
    loop {
        let field = multipart
            .next_field()
            .await
            .map_err(|_| AppError::bad_request("multipart body is unreadable"))?;
        let Some(field) = field else {
            break;
        };
        match field.name() {
            Some("file") if audio.is_none() => {
                audio = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|_| AppError::bad_request("audio part is unreadable"))?,
                );
            }
            Some("model") => {
                model = field
                    .text()
                    .await
                    .ok()
                    .map(|model| model.trim().to_owned())
                    .filter(|model| !model.is_empty());
            }
            _ => {}
        }
    }
    let bytes = audio.ok_or_else(|| {
        AppError::bad_request("multipart body must carry the audio in the file field")
    })?;
    if bytes.is_empty() {
        return Err(AppError::bad_request("the audio file is empty"));
    }
    if bytes.len() > MAX_AUDIO_BYTES {
        return Err(AppError::payload_too_large(
            format!(
                "the audio file is {} bytes; DeepSeek transcribes at most {MAX_AUDIO_BYTES} bytes per request",
                bytes.len()
            ),
            "audio_too_large",
        ));
    }
    let container = accepted_container(&bytes)?;
    Ok((AudioUpload { bytes, container }, model))
}

/// The container `bytes` prove, or the 400 that tells the caller what
/// DeepSeek does take. A WAV whose header describes anything but 16-bit PCM is
/// rejected here, because DeepSeek reads the header and refuses it anyway.
fn accepted_container(bytes: &[u8]) -> Result<Container, AppError> {
    if let Some(wav) = wav_header(bytes) {
        if wav.audio_format != 1 || wav.bits_per_sample != 16 {
            return Err(AppError::bad_request(format!(
                "DeepSeek transcribes 16-bit PCM WAV only; this file is {}-bit {}",
                wav.bits_per_sample,
                if wav.audio_format == 1 {
                    "PCM"
                } else {
                    "compressed"
                }
            )));
        }
        if let Some(duration_ms) = wav.duration_ms
            && duration_ms > MAX_AUDIO_MS
        {
            return Err(AppError::bad_request(format!(
                "the recording is {} ms of audio; DeepSeek transcribes at most {MAX_AUDIO_MS} ms per request",
                duration_ms
            )));
        }
        return Ok(Container::Wav);
    }
    if is_ogg_opus(bytes) {
        return Ok(Container::OggOpus);
    }
    Err(AppError::bad_request(
        "DeepSeek transcribes 16-bit PCM WAV or Ogg Opus audio; this file is neither",
    ))
}

/// The upstream `multipart/form-data` body: one `file` part and nothing else.
pub fn upstream_body(upload: &AudioUpload, boundary: &str) -> Vec<u8> {
    let container = upload.container;
    let mut body = Vec::with_capacity(upload.bytes.len() + 256);
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
            container.file_name()
        )
        .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {}\r\n\r\n", container.media_type()).as_bytes());
    body.extend_from_slice(&upload.bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

/// The content type the upstream request presents for `boundary`.
pub fn upstream_content_type(boundary: &str) -> String {
    format!("multipart/form-data; boundary={boundary}")
}

/// The audio duration DeepSeek reports for one completed transcription, which
/// is what the call is billed on.
pub fn duration_ms(response: &Value) -> Result<i64, AppError> {
    response
        .get("duration_ms")
        .and_then(Value::as_i64)
        .filter(|duration_ms| *duration_ms >= 0)
        .ok_or_else(|| {
            AppError::upstream_with_reason(
                502,
                "asr_data_missing",
                "Transcription failed: the DeepSeek response carried no duration.",
            )
        })
}

/// The transcript DeepSeek returned, refused when it is empty: a caller asked
/// for text and got none.
pub fn transcript(response: &Value) -> Result<&str, AppError> {
    response
        .get("text")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| {
            AppError::upstream_with_reason(
                502,
                "asr_data_missing",
                "Transcription failed: the DeepSeek response carried no text.",
            )
        })
}

struct WavHeader {
    audio_format: u16,
    bits_per_sample: u16,
    /// Duration from the `data` chunk and the byte rate, when both are present.
    duration_ms: Option<i64>,
}

/// Reads the RIFF chunks of a WAV file; `None` when the bytes are not a WAV at
/// all. Chunks that are missing or truncated keep the corresponding field
/// unknown rather than failing: DeepSeek decides on the file itself, and only
/// the checks that can be made locally are made.
fn wav_header(bytes: &[u8]) -> Option<WavHeader> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut audio_format = None;
    let mut byte_rate = None;
    let mut bits_per_sample = None;
    let mut data_bytes = None;
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize;
        let payload = offset + 8;
        match id {
            b"fmt " if payload + 16 <= bytes.len() => {
                audio_format = Some(u16::from_le_bytes(
                    bytes[payload..payload + 2].try_into().ok()?,
                ));
                byte_rate = Some(u32::from_le_bytes(
                    bytes[payload + 8..payload + 12].try_into().ok()?,
                ));
                bits_per_sample = Some(u16::from_le_bytes(
                    bytes[payload + 14..payload + 16].try_into().ok()?,
                ));
            }
            b"data" => {
                // A streamed WAV may declare a placeholder size; the bytes that
                // are actually here are the better bound.
                let declared = size.min(bytes.len().saturating_sub(payload));
                data_bytes = Some(declared);
                break;
            }
            _ => {}
        }
        // Chunks are word aligned: an odd size carries one pad byte.
        offset = payload + size + (size & 1);
    }
    let duration_ms = match (byte_rate, data_bytes) {
        (Some(0), _) | (None, _) | (_, None) => None,
        (Some(byte_rate), Some(data_bytes)) => Some(
            (data_bytes as i64)
                .saturating_mul(1_000)
                .saturating_div(i64::from(byte_rate)),
        ),
    };
    Some(WavHeader {
        audio_format: audio_format?,
        bits_per_sample: bits_per_sample?,
        duration_ms,
    })
}

/// Whether the bytes are an Ogg container whose first packet is Opus.
fn is_ogg_opus(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && &bytes[0..4] == b"OggS" && {
        let head = &bytes[..bytes.len().min(64)];
        head.windows(8).any(|window| window == b"OpusHead")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A minimal 16-bit PCM WAV: `seconds` of 16 kHz mono silence.
    fn wav(seconds: u32, audio_format: u16, bits: u16) -> Vec<u8> {
        let sample_rate = 16_000u32;
        let channels = 1u16;
        let block_align = channels * bits / 8;
        let byte_rate = sample_rate * u32::from(block_align);
        let data_bytes = byte_rate * seconds;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_bytes).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&audio_format.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&byte_rate.to_le_bytes());
        bytes.extend_from_slice(&block_align.to_le_bytes());
        bytes.extend_from_slice(&bits.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_bytes.to_le_bytes());
        bytes.resize(bytes.len() + data_bytes as usize, 0);
        bytes
    }

    fn multipart(boundary: &str, file: &[u8], model: &str) -> (String, Bytes) {
        let body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"clip.webm\"\r\nContent-Type: audio/wav\r\n\r\n"
        )
        .into_bytes()
        .into_iter()
        .chain(file.iter().copied())
        .chain(
            format!(
                "\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n{model}\r\n--{boundary}--\r\n"
            )
            .into_bytes(),
        )
        .collect::<Vec<_>>();
        (
            format!("multipart/form-data; boundary={boundary}"),
            Bytes::from(body),
        )
    }

    #[test]
    fn wav_header_reports_the_duration_and_the_sample_format() {
        let header = wav_header(&wav(3, 1, 16)).expect("a WAV header");
        assert_eq!(header.audio_format, 1);
        assert_eq!(header.bits_per_sample, 16);
        assert_eq!(header.duration_ms, Some(3_000));
    }

    #[test]
    fn non_wav_audio_is_not_mistaken_for_a_wav() {
        assert!(wav_header(b"OggS\0\0\0\0\0\0\0\0").is_none());
        assert!(wav_header(b"not audio").is_none());
    }

    #[test]
    fn ogg_opus_is_detected_from_the_first_page() {
        let mut bytes = b"OggS\0\x02\0\0\0\0\0\0\0\0".to_vec();
        bytes.extend_from_slice(b"\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
        bytes.extend_from_slice(b"\x13OpusHead\x01\x02");
        assert!(is_ogg_opus(&bytes));
        // Ogg Vorbis is an Ogg but not an Opus.
        let mut vorbis = b"OggS\0\x02\0\0\0\0\0\0\0\0".to_vec();
        vorbis.extend_from_slice(&[0u8; 32]);
        vorbis.extend_from_slice(b"\x01vorbis");
        assert!(!is_ogg_opus(&vorbis));
    }

    #[test]
    fn validation_accepts_sixteen_bit_pcm_wav_and_ogg_opus() {
        let mut opus = b"OggS\0\x02\0\0\0\0\0\0\0\0".to_vec();
        opus.extend_from_slice(&[0u8; 32]);
        opus.extend_from_slice(b"\x13OpusHead\x01\x02");
        assert_eq!(
            accepted_container(&wav(1, 1, 16)).ok(),
            Some(Container::Wav)
        );
        assert_eq!(accepted_container(&opus).ok(), Some(Container::OggOpus));
    }

    #[test]
    fn validation_names_the_accepted_formats_and_the_limits() {
        let error = accepted_container(b"\x1aE\xdf\xa3 webm bytes").expect_err("webm is refused");
        assert_eq!(error.status(), axum::http::StatusCode::BAD_REQUEST);
        assert!(error.message().contains("16-bit PCM WAV or Ogg Opus"));

        let error = accepted_container(&wav(1, 1, 8)).expect_err("8-bit PCM is refused");
        assert!(error.message().contains("8-bit PCM"));

        let error = accepted_container(&wav(121, 1, 16)).expect_err("121 s is too long");
        assert!(error.message().contains("120000 ms"));
    }

    #[tokio::test]
    async fn an_oversized_upload_is_read_as_a_413() {
        let bytes = vec![0u8; MAX_AUDIO_BYTES + 1];
        let (content_type, body) = multipart("b1", &bytes, "deepseek-asr");
        let error = read_upload(&content_type, body)
            .await
            .expect_err("an oversized upload is refused");
        assert_eq!(error.status(), axum::http::StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn the_upload_is_read_into_one_field_with_the_caller_model_beside_it() {
        let wav = wav(2, 1, 16);
        let (content_type, body) = multipart("b2", &wav, "deepseek-asr");
        let (upload, model) = read_upload(&content_type, body)
            .await
            .expect("the WAV upload is accepted");
        assert_eq!(upload.container, Container::Wav);
        assert_eq!(upload.bytes.as_ref(), wav.as_slice());
        assert_eq!(model.as_deref(), Some("deepseek-asr"));
    }

    #[tokio::test]
    async fn a_body_without_a_file_field_is_rejected() {
        let content_type = "multipart/form-data; boundary=b3".to_owned();
        let body = Bytes::from(
            "--b3\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\ndeepseek-asr\r\n--b3--\r\n",
        );
        let error = read_upload(&content_type, body)
            .await
            .expect_err("a body without audio is refused");
        assert!(error.message().contains("file field"));
        let error = read_upload("application/json", Bytes::new())
            .await
            .expect_err("a non-multipart body is refused");
        assert!(error.message().contains("multipart/form-data"));
    }

    #[test]
    fn the_upstream_body_carries_exactly_one_file_part() {
        let upload = AudioUpload {
            bytes: Bytes::from_static(b"wav-bytes"),
            container: Container::Wav,
        };
        let body = String::from_utf8(upstream_body(&upload, "boundary-x")).expect("utf-8 body");
        assert_eq!(
            body,
            "--boundary-x\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\nwav-bytes\r\n--boundary-x--\r\n"
        );
        assert_eq!(
            upstream_content_type("boundary-x"),
            "multipart/form-data; boundary=boundary-x"
        );
    }

    #[test]
    fn a_response_without_a_transcript_or_duration_is_an_upstream_failure() {
        assert_eq!(
            duration_ms(&json!({"text":"hi","duration_ms":3200})).ok(),
            Some(3_200)
        );
        assert_eq!(
            transcript(&json!({"text":"hi","duration_ms":3200})).ok(),
            Some("hi")
        );
        assert_eq!(
            duration_ms(&json!({"text":"hi"}))
                .expect_err("a duration is required")
                .reason(),
            Some("asr_data_missing")
        );
        assert_eq!(
            transcript(&json!({"text":"   ","duration_ms":1}))
                .expect_err("an empty transcript is refused")
                .reason(),
            Some("asr_data_missing")
        );
    }
}
