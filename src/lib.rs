pub mod domain;
pub mod infrastructure;
pub mod utils {
    pub mod env;
}

pub mod ipc;
pub mod native;

pub use utils::env::load_env;
