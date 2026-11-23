mod google;
mod model;
mod openai;
pub mod schema;

pub use ai_derive::StructuredOutput;
pub use google::DEFAULT_MODEL as DEFAULT_GEMINI_MODEL;
pub use model::{
    Attempt, ChainError, Generated, Llm, Mllm, Model, ModelChain, ModelError, ModelRef, ModelSpec,
    Provider, StructuredOutput, Vllm,
};
