pub mod audio;
pub mod dictionary_service;
pub mod recording_service;
pub mod transcription_service;

pub use audio::{
    AudioBackend, AudioBackendError, AudioData, AudioDataFormat, AudioFrame, CapturedAudio,
    Recorder,
};
pub use dictionary_service::{DictRepository, DictionaryService};
pub use recording_service::{
    ActiveRecordingSession, RecordedAudio, RecordingConfig, RecordingContext, RecordingOptions,
    RecordingService, RecordingState, StopCaptureOutcome, StopRecordingOutcome,
    StoppedSessionContext,
};
pub use transcription_service::{
    TranscriptionClient, TranscriptionClientError, TranscriptionClientOptions, TranscriptionEvent,
    TranscriptionLogEntry, TranscriptionLogWriter, TranscriptionOptions, TranscriptionService,
};
