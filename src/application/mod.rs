pub mod command_handler;
pub mod media_control_service;
pub mod recording_service;
pub mod service_container;
pub mod traits;
pub mod transcription_service;
pub mod transcription_worker;

pub use command_handler::{CommandHandler, TranscriptionMessage};
pub use media_control_service::MediaControlService;
pub use recording_service::{
    RecordingConfig, RecordingContext, RecordingOptions, RecordingService, RecordingState,
};
pub use service_container::{AppConfig, ServiceContainer};
pub use transcription_service::{TranscriptionOptions, TranscriptionService};
pub use transcription_worker::{handle_transcription, spawn_transcription_worker};
