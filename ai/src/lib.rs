mod google;
mod model;
mod openai;
pub mod schema;

pub use ai_derive::StructuredOutput;
pub use model::{
    Attempt, ChainError, Generated, Llm, Mllm, Model, ModelChain, ModelError, ModelRef, ModelSpec,
    Provider, StructuredOutput, Vllm,
};
