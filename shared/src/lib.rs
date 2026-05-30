pub mod protocol;
pub mod database;

mod todo;
mod finances;
mod purchases;


// Re-export derive macros
pub use maia_macros::{Crud};
