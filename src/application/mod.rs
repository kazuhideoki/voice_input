pub mod audio;
pub mod dictionary_service;
pub mod recording_service;
pub mod transcription_service;

pub use audio::{AudioBackend, AudioBackendError, AudioData, Recorder};
pub use dictionary_service::{DictRepository, DictionaryService};
pub use recording_service::{
    ActiveRecordingSession, RecordedAudio, RecordingConfig, RecordingContext, RecordingOptions,
    RecordingService, RecordingState, StopRecordingOutcome, StoppedSessionContext,
};
pub use transcription_service::{
    TranscriptionClient, TranscriptionClientError, TranscriptionEvent, TranscriptionLogEntry,
    TranscriptionLogWriter, TranscriptionOptions, TranscriptionService,
};
