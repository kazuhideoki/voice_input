pub mod cli;
pub mod domain;
pub mod infrastructure;
pub mod utils {
    pub mod detach;
}

// Re-export the spawn_detached function
pub use utils::detach::spawn_detached;

// Temporary main implementation for Phase 1
// This will be removed in later phases when we properly implement the CLI layer
pub fn main() {
    println!("Voice Input CLI - Phase 1 restructuring in progress");
    println!("This is a placeholder in lib.rs. Full functionality will be restored in Phase 2-5.");
}
