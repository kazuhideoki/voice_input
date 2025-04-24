pub mod utils {
    pub mod detach;
}

// Re-export the spawn_detached function
pub use utils::detach::spawn_detached;
