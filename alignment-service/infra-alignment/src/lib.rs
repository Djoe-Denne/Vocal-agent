use alignment_domain::{
    AlignmentOutput, AlignmentPort, AlignmentRequest, DomainError, WordTiming,
};
use async_trait::async_trait;
use wav2vec2_rs::{AlignmentError, AlignmentInput, Wav2Vec2Aligner, Wav2Vec2Config};

#[derive(Debug, Clone)]
pub struct Wav2Vec2AdapterConfig {
    pub model_path: String,
    pub config_path: String,
    pub vocab_path: String,
    pub device: String,
}

pub struct Wav2Vec2ForcedAligner {
    aligner: Wav2Vec2Aligner,
}

impl Wav2Vec2ForcedAligner {
    pub fn load(adapter_cfg: &Wav2Vec2AdapterConfig) -> Result<Self, DomainError> {
        let core_cfg = Wav2Vec2Config {
            model_path: adapter_cfg.model_path.clone(),
            config_path: adapter_cfg.config_path.clone(),
            vocab_path: adapter_cfg.vocab_path.clone(),
            device: adapter_cfg.device.clone(),
            expected_sample_rate_hz: Wav2Vec2Config::DEFAULT_SAMPLE_RATE_HZ,
        };

        let aligner = Wav2Vec2Aligner::load(&core_cfg).map_err(Self::map_error)?;
        Ok(Self { aligner })
    }

    fn map_error(error: AlignmentError) -> DomainError {
        match error {
            AlignmentError::InvalidInput { message } => DomainError::invalid_input(&message),
            other => DomainError::internal_error(&other.to_string()),
        }
    }
}

#[async_trait]
impl AlignmentPort for Wav2Vec2ForcedAligner {
    async fn align(&self, request: AlignmentRequest) -> Result<AlignmentOutput, DomainError> {
        let transcript_text = request
            .transcript
            .segments
            .iter()
            .map(|segment| segment.text.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join(" ");

        let output = self
            .aligner
            .align(&AlignmentInput {
                sample_rate_hz: request.audio.sample_rate_hz,
                samples: request.audio.samples,
                transcript: transcript_text,
            })
            .map_err(Self::map_error)?;

        Ok(AlignmentOutput {
            words: output
                .words
                .into_iter()
                .map(|word| WordTiming {
                    word: word.word,
                    start_ms: word.start_ms,
                    end_ms: word.end_ms,
                    confidence: word.confidence,
                })
                .collect(),
        })
    }
}
