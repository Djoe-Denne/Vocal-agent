use std::{io::Cursor, time::Duration};

use async_trait::async_trait;
use orchestration_domain::{
    DomainError, PipelineContext, PipelineStage, SynthesizedWordTiming, TtsOutput,
};
use reqwest::Client;
use serde::Serialize;
use serde_json::json;

pub struct TtsRestSynthesizeStage {
    client: Client,
    endpoint_uri: String,
    request_timeout: Duration,
}

impl TtsRestSynthesizeStage {
    pub fn new(endpoint_uri: impl Into<String>, request_timeout: Duration) -> Self {
        Self {
            client: Client::new(),
            endpoint_uri: endpoint_uri.into(),
            request_timeout,
        }
    }
}

#[derive(Serialize)]
struct TtsSpeechRequest<'a> {
    input: &'a str,
}

#[async_trait]
impl PipelineStage for TtsRestSynthesizeStage {
    fn name(&self) -> &'static str {
        "tts_synthesize"
    }

    async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
        let input_text = transcript_text(context)?;
        let payload = TtsSpeechRequest { input: &input_text };

        let response = tokio::time::timeout(
            self.request_timeout,
            self.client.post(&self.endpoint_uri).json(&payload).send(),
        )
        .await
        .map_err(|_| DomainError::external_service_error("tts", "HTTP request timed out"))?
        .map_err(|err| {
            DomainError::external_service_error("tts", &format!("HTTP request failed: {err}"))
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DomainError::external_service_error(
                "tts",
                &format!(
                    "HTTP {} from {}: {}",
                    status.as_u16(),
                    self.endpoint_uri,
                    truncate_text(&body, 300)
                ),
            ));
        }

        let wav_bytes = tokio::time::timeout(self.request_timeout, response.bytes())
            .await
            .map_err(|_| {
                DomainError::external_service_error("tts", "timed out reading HTTP response body")
            })?
            .map_err(|err| {
                DomainError::external_service_error(
                    "tts",
                    &format!("failed reading HTTP response body: {err}"),
                )
            })?;

        let (samples, sample_rate_hz) = decode_wav_to_mono_f32(wav_bytes.as_ref())?;
        let word_timings = collect_word_timings(context);
        let tts_output = TtsOutput {
            samples,
            sample_rate_hz,
            word_timings,
        };

        tracing::debug!(
            sample_rate_hz = tts_output.sample_rate_hz,
            sample_count = tts_output.samples.len(),
            "tts_synthesize: received new audio"
        );
        context.set_extension("tts.sample_count", json!(tts_output.samples.len()));
        context.set_extension("tts.sample_rate_hz", json!(tts_output.sample_rate_hz));
        context.set_extension("tts.endpoint_uri", json!(self.endpoint_uri));
        context.tts_output = Some(tts_output);
        Ok(())
    }
}

fn transcript_text(context: &PipelineContext) -> Result<String, DomainError> {
    let transcript = context
        .transcript
        .as_ref()
        .ok_or_else(|| DomainError::internal_error("no transcript available for tts stage"))?;
    let text = transcript
        .segments
        .iter()
        .map(|segment| segment.text.trim())
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if text.is_empty() {
        return Err(DomainError::internal_error(
            "cannot synthesize empty transcript text",
        ));
    }
    Ok(text)
}

fn collect_word_timings(context: &PipelineContext) -> Vec<SynthesizedWordTiming> {
    if !context.aligned_words.is_empty() {
        return context
            .aligned_words
            .iter()
            .map(|word| SynthesizedWordTiming {
                text: word.word.clone(),
                start_ms: word.start_ms,
                end_ms: word.end_ms,
                fit_strategy: "alignment".to_string(),
            })
            .collect();
    }

    context
        .transcript
        .as_ref()
        .map(|transcript| {
            transcript
                .segments
                .iter()
                .flat_map(|segment| segment.tokens.iter())
                .map(|token| SynthesizedWordTiming {
                    text: token.text.clone(),
                    start_ms: token.start_ms,
                    end_ms: token.end_ms,
                    fit_strategy: "token".to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn decode_wav_to_mono_f32(bytes: &[u8]) -> Result<(Vec<f32>, u32), DomainError> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes)).map_err(|err| {
        DomainError::external_service_error("tts", &format!("invalid WAV response: {err}"))
    })?;
    let spec = reader.spec();
    if spec.channels == 0 {
        return Err(DomainError::external_service_error(
            "tts",
            "invalid WAV response: channel count is zero",
        ));
    }

    let channels = spec.channels as usize;
    let interleaved = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| {
                DomainError::external_service_error(
                    "tts",
                    &format!("failed to decode float WAV samples: {err}"),
                )
            })?,
        hound::SampleFormat::Int => decode_int_samples(&mut reader, spec.bits_per_sample)?,
    };

    let samples = downmix_to_mono(interleaved, channels);
    if samples.is_empty() {
        return Err(DomainError::external_service_error(
            "tts",
            "WAV response contained no samples",
        ));
    }
    Ok((samples, spec.sample_rate))
}

fn decode_int_samples<R: std::io::Read + std::io::Seek>(
    reader: &mut hound::WavReader<R>,
    bits_per_sample: u16,
) -> Result<Vec<f32>, DomainError> {
    if bits_per_sample == 0 || bits_per_sample > 32 {
        return Err(DomainError::external_service_error(
            "tts",
            &format!("unsupported integer WAV bit depth: {bits_per_sample}"),
        ));
    }

    if bits_per_sample <= 16 {
        return reader
            .samples::<i16>()
            .map(|sample| {
                sample.map(|value| value as f32 / i16::MAX as f32).map_err(|err| {
                    DomainError::external_service_error(
                        "tts",
                        &format!("failed to decode 16-bit WAV samples: {err}"),
                    )
                })
            })
            .collect();
    }

    let max_amplitude = ((1_i64 << u32::from(bits_per_sample - 1)) - 1) as f32;
    reader
        .samples::<i32>()
        .map(|sample| {
            sample
                .map(|value| value as f32 / max_amplitude)
                .map_err(|err| {
                    DomainError::external_service_error(
                        "tts",
                        &format!("failed to decode {}-bit WAV samples: {err}", bits_per_sample),
                    )
                })
        })
        .collect()
}

fn downmix_to_mono(interleaved: Vec<f32>, channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved;
    }
    interleaved
        .chunks(channels)
        .map(|frame| frame.iter().copied().sum::<f32>() / frame.len() as f32)
        .collect()
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    format!("{}...", value.chars().take(max_chars).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchestration_domain::{
        LanguageTag, PipelineContext, Transcript, TranscriptSegment, TranscriptToken,
    };

    #[test]
    fn request_payload_serializes_as_input_only() {
        let payload = TtsSpeechRequest {
            input: "Ceci est un test de serveur TTS CUDA.",
        };
        let serialized = serde_json::to_string(&payload).expect("payload should serialize");
        assert_eq!(
            serialized,
            r#"{"input":"Ceci est un test de serveur TTS CUDA."}"#
        );
        let value = serde_json::to_value(&payload).expect("payload should serialize to value");
        assert_eq!(
            value,
            json!({ "input": "Ceci est un test de serveur TTS CUDA." })
        );
    }

    #[test]
    fn wav_decoder_reads_float_wav() {
        let wav = build_test_wav_f32();
        let (samples, sample_rate_hz) = decode_wav_to_mono_f32(&wav).expect("wav should decode");
        assert_eq!(sample_rate_hz, 22_050);
        assert_eq!(samples.len(), 3);
        assert!(samples[0].abs() < 1e-6);
    }

    #[test]
    fn transcript_text_joins_non_empty_segments() {
        let mut context = PipelineContext::new("s2", None);
        context.transcript = Some(sample_transcript());
        assert_eq!(transcript_text(&context).unwrap_or_default(), "hello world");
    }

    fn sample_transcript() -> Transcript {
        Transcript {
            language: LanguageTag::En,
            segments: vec![
                TranscriptSegment {
                    text: "hello".to_string(),
                    start_ms: 0,
                    end_ms: 100,
                    tokens: vec![TranscriptToken {
                        text: "hello".to_string(),
                        start_ms: 0,
                        end_ms: 100,
                        confidence: 0.99,
                    }],
                },
                TranscriptSegment {
                    text: "world".to_string(),
                    start_ms: 100,
                    end_ms: 200,
                    tokens: vec![TranscriptToken {
                        text: "world".to_string(),
                        start_ms: 100,
                        end_ms: 200,
                        confidence: 0.98,
                    }],
                },
            ],
        }
    }

    fn build_test_wav_f32() -> Vec<u8> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 22_050,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(&mut cursor, spec).expect("create writer");
            writer.write_sample(0.0f32).expect("sample");
            writer.write_sample(0.25f32).expect("sample");
            writer.write_sample(-0.25f32).expect("sample");
            writer.finalize().expect("finalize");
        }
        cursor.into_inner()
    }
}
