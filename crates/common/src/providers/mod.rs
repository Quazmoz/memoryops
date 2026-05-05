pub mod http;
pub mod local;
pub mod traits;

pub use http::{
    AnthropicProvider, OllamaProvider, OpenAIEmbedProvider, OpenAIProvider,
    OpenAiCompatibleProvider,
};
pub use local::FastEmbedProvider;
pub use traits::{EmbeddingProvider, LlmProvider};
