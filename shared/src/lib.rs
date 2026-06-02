pub mod database;
pub mod protocol;

mod finances;
mod purchases;
mod todo;

// Re-export derive macros
pub use maia_macros::Crud;
