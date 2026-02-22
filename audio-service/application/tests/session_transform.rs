use std::sync::Arc;

use audio_application::{
    TransformAudioCommand, TransformAudioCommandHandler, TransformAudioRequest,
    TransformAudioUseCase, TransformAudioUseCaseImpl,
};
use audio_domain::AudioTransformPort;
use audio_infra::AudioTransformerAdapter;
use rustycog_command::CommandHandler;

#[tokio::test]
async fn transform_command_flow_produces_resampled_audio() {
    let transformer: Arc<dyn AudioTransformPort> = Arc::new(AudioTransformerAdapter::new());
    let usecase: Arc<dyn TransformAudioUseCase> =
        Arc::new(TransformAudioUseCaseImpl::new(transformer, 16_000));
    let handler = TransformAudioCommandHandler::new(usecase);

    let response = handler
        .handle(TransformAudioCommand::new(TransformAudioRequest {
            samples: (0..480).map(|i| i as f32 / 480.0).collect(),
            sample_rate_hz: Some(48_000),
            target_sample_rate_hz: Some(16_000),
            session_id: Some("it-session".to_string()),
        }))
        .await
        .expect("command succeeds");

    assert_eq!(response.session_id, "it-session");
    assert_eq!(response.sample_rate_hz, 16_000);
    assert!(response.metadata.resampled);
    assert!(response.samples.len() < 480);
}
