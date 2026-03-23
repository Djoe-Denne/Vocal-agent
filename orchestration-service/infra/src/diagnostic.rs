use std::io::Write;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use orchestration_domain::{DomainError, PipelineContext, PipelineStage, WordTiming};
use serde::Serialize;

pub struct DiagnosticDumpStage {
    label: String,
    output_dir: PathBuf,
}

impl DiagnosticDumpStage {
    pub fn new(label: impl Into<String>, output_dir: impl Into<PathBuf>) -> Self {
        Self {
            label: label.into(),
            output_dir: output_dir.into(),
        }
    }

    fn session_dir(&self, session_id: &str) -> PathBuf {
        self.output_dir.join(session_id)
    }
}

#[async_trait]
impl PipelineStage for DiagnosticDumpStage {
    fn name(&self) -> &'static str {
        "diagnostic_dump"
    }

    async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
        let dir = self.session_dir(&context.session_id);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!(error = %e, dir = %dir.display(), "failed to create diagnostic dump directory");
            return Ok(());
        }

        let prefix = &self.label;
        let audio_path = dir.join(format!("{prefix}_audio.wav"));
        let timings_path = dir.join(format!("{prefix}_timings.json"));
        let transcript_path = dir.join(format!("{prefix}_transcript.txt"));
        let summary_path = dir.join(format!("{prefix}_summary.json"));

        if let Err(e) = write_wav(
            &audio_path,
            &context.audio.samples,
            context.audio.sample_rate_hz,
        ) {
            tracing::warn!(error = %e, path = %audio_path.display(), "failed to write diagnostic WAV");
        }

        if let Err(e) = write_timings(&timings_path, &context.aligned_words) {
            tracing::warn!(error = %e, path = %timings_path.display(), "failed to write diagnostic timings");
        }

        let transcript_text = context
            .transcript
            .as_ref()
            .map(|t| {
                t.segments
                    .iter()
                    .map(|s| s.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        if let Err(e) = std::fs::write(&transcript_path, &transcript_text) {
            tracing::warn!(error = %e, path = %transcript_path.display(), "failed to write diagnostic transcript");
        }

        let summary = DumpSummary {
            label: prefix.clone(),
            session_id: context.session_id.clone(),
            sample_count: context.audio.samples.len(),
            sample_rate_hz: context.audio.sample_rate_hz,
            duration_ms: if context.audio.sample_rate_hz > 0 {
                (context.audio.samples.len() as u64 * 1000)
                    / context.audio.sample_rate_hz as u64
            } else {
                0
            },
            word_count: context.aligned_words.len(),
            transcript_len: transcript_text.len(),
            has_tts_output: context.tts_output.is_some(),
            tts_sample_count: context.tts_output.as_ref().map(|t| t.samples.len()),
            tts_sample_rate_hz: context.tts_output.as_ref().map(|t| t.sample_rate_hz),
            extension_keys: context.extensions.keys().cloned().collect(),
        };
        if let Err(e) = write_json(&summary_path, &summary) {
            tracing::warn!(error = %e, path = %summary_path.display(), "failed to write diagnostic summary");
        }

        if let Some(tts) = &context.tts_output {
            let tts_wav_path = dir.join(format!("{prefix}_tts_raw.wav"));
            if let Err(e) = write_wav(&tts_wav_path, &tts.samples, tts.sample_rate_hz) {
                tracing::warn!(error = %e, path = %tts_wav_path.display(), "failed to write TTS WAV");
            }
        }

        tracing::info!(
            label = %prefix,
            session_id = %context.session_id,
            sample_count = context.audio.samples.len(),
            sample_rate_hz = context.audio.sample_rate_hz,
            duration_ms = summary.duration_ms,
            word_count = context.aligned_words.len(),
            dump_dir = %dir.display(),
            "diagnostic dump written"
        );

        Ok(())
    }
}

#[derive(Serialize)]
struct DumpSummary {
    label: String,
    session_id: String,
    sample_count: usize,
    sample_rate_hz: u32,
    duration_ms: u64,
    word_count: usize,
    transcript_len: usize,
    has_tts_output: bool,
    tts_sample_count: Option<usize>,
    tts_sample_rate_hz: Option<u32>,
    extension_keys: Vec<String>,
}

fn write_wav(path: &Path, samples: &[f32], sample_rate_hz: u32) -> std::io::Result<()> {
    let num_channels: u16 = 1;
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate_hz * u32::from(num_channels) * u32::from(bits_per_sample) / 8;
    let block_align = num_channels * bits_per_sample / 8;
    let data_size = samples.len() as u32 * u32::from(block_align);
    let file_size = 36 + data_size;

    let mut f = std::fs::File::create(path)?;
    f.write_all(b"RIFF")?;
    f.write_all(&file_size.to_le_bytes())?;
    f.write_all(b"WAVE")?;
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&num_channels.to_le_bytes())?;
    f.write_all(&sample_rate_hz.to_le_bytes())?;
    f.write_all(&byte_rate.to_le_bytes())?;
    f.write_all(&block_align.to_le_bytes())?;
    f.write_all(&bits_per_sample.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_size.to_le_bytes())?;

    for &sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let pcm = (clamped * 32767.0) as i16;
        f.write_all(&pcm.to_le_bytes())?;
    }

    Ok(())
}

fn write_timings(path: &Path, words: &[WordTiming]) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(words).unwrap_or_else(|_| "[]".to_string());
    std::fs::write(path, json)
}

fn write_json(path: &Path, value: &impl Serialize) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string());
    std::fs::write(path, json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchestration_domain::PipelineContext;

    #[tokio::test]
    async fn diagnostic_dump_creates_files() {
        let tmp = std::env::temp_dir().join("vocaloid-diag-test");
        let _ = std::fs::remove_dir_all(&tmp);

        let stage = DiagnosticDumpStage::new("01_original", &tmp);
        let mut context = PipelineContext::new("test-session", None);
        context.audio.samples = vec![0.1, 0.2, 0.3, 0.4];
        context.audio.sample_rate_hz = 16_000;
        context.aligned_words = vec![WordTiming {
            word: "hello".to_string(),
            start_ms: 0,
            end_ms: 250,
            confidence: 0.95,
        }];

        stage.execute(&mut context).await.expect("dump should succeed");

        let session_dir = tmp.join("test-session");
        assert!(session_dir.join("01_original_audio.wav").exists());
        assert!(session_dir.join("01_original_timings.json").exists());
        assert!(session_dir.join("01_original_transcript.txt").exists());
        assert!(session_dir.join("01_original_summary.json").exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
