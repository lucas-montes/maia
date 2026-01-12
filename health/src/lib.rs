// Re-export health models from shared
pub use shared::models::health::*;

// Health-specific service logic will go here
pub mod services {
    // TODO: Implement service layer
}

#[cfg(feature = "cli")]
pub mod commands {
    // TODO: Implement CLI commands
}
