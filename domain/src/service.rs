use crate::Transcript;

pub trait AsrCapabilityService: Send + Sync {
    fn supported_languages(&self) -> &[String];
    fn default_language(&self) -> &str;
    fn transcript_text(&self, transcript: &Transcript) -> String {
        transcript
            .segments
            .iter()
            .map(|segment| segment.text.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}
