pub mod domain;
pub mod infrastructure;
pub mod utils {
    pub mod detach;
    pub mod env;
}

pub mod ipc;
pub use utils::detach::spawn_detached;
pub use utils::env::load_env;
