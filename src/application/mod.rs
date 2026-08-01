pub mod audio;
pub mod config_defaults;
pub mod dictionary_service;
pub mod history_service;
pub mod recording_service;
pub mod transcription_service;

pub use audio::{
    AudioBackend, AudioBackendError, AudioData, AudioDataFormat, AudioFrame, AudioInputSource,
    CapturedAudio, Recorder,
};
pub use config_defaults::TranscriptionProvider;
pub use dictionary_service::{DictRepository, DictionaryService};
pub use history_service::{TranscriptionHistoryEntry, TranscriptionHistoryService};
pub use recording_service::{
    ActiveRecordingSession, RecordedAudio, RecordingConfig, RecordingContext, RecordingOptions,
    RecordingService, RecordingState, StopCaptureOutcome, StopRecordingOutcome,
    StoppedSessionContext,
};
pub use transcription_service::{
    TranscriptionClient, TranscriptionClientError, TranscriptionClientOptions, TranscriptionEvent,
    TranscriptionLogEntry, TranscriptionLogWriter, TranscriptionOptions, TranscriptionService,
};
