pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod utils {
    pub mod env;
}
pub mod monitoring;
pub mod shortcut;

pub mod cli;
pub mod ipc;
pub use utils::env::load_env;
