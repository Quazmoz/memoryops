# LLM & Embedding Provider Configuration

MemoryOps uses two pluggable provider traits:

- **`LlmProvider`** — used for memory summarization and slow-path processing (`crates/common/src/providers/traits.rs`)
- **`EmbeddingProvider`** — used for generating vector embeddings at ingestion and retrieval time

All provider selection and configuration is done in `config.toml`. API keys are **always** read from environment variables — never stored in `config.toml`.

---

## LLM Providers

Set `llm.provider` in `config.toml` to one of the values below.

### Ollama (default — fully local)

```toml
[llm]
provider = "ollama"
model = "llama3"
base_url = "http://localhost:11434"
timeout_secs = 120
```

No API key required. Ollama uses `/api/generate` (not the OpenAI-compatible endpoint). Pull the model first: `ollama pull llama3`.

Optional Bearer auth (for proxied Ollama):

```toml
[llm.ollama]
api_key_env = "OLLAMA_API_KEY"  # env var name, not value
```

---

### OpenAI

```toml
[llm]
provider = "openai"
model = "gpt-4o-mini"
```

```bash
# .env
OPENAI_API_KEY=sk-...
```

---

### Anthropic

```toml
[llm]
provider = "anthropic"
model = "claude-3-5-haiku-20241022"
```

```bash
# .env
ANTHROPIC_API_KEY=sk-ant-...
```

---

### Google Gemini

```toml
[llm]
provider = "gemini"
model = "gemini-1.5-flash"

[llm.gemini]
api_key_env = "GEMINI_API_KEY"
```

```bash
# .env
GEMINI_API_KEY=AIza...
```

Gemini uses Google's native REST API (`generativelanguage.googleapis.com`), not the OpenAI shim.

---

### OpenRouter

Access hundreds of models (GPT-4o, Claude, Mistral, Llama, etc.) via a single endpoint.

```toml
[llm]
provider = "openrouter"
model = "mistralai/mistral-7b-instruct"
base_url = ""  # leave empty to use https://openrouter.ai/api/v1

[llm.openai_compatible]
api_key_env = "OPENROUTER_API_KEY"

# Optional: OpenRouter recommends these headers for usage tracking
[llm.openai_compatible.headers]
"HTTP-Referer" = "https://github.com/Quazmoz/memoryops"
"X-Title" = "MemoryOps"
```

```bash
# .env
OPENROUTER_API_KEY=sk-or-...
```

OpenRouter uses the OpenAI-compatible `/chat/completions` endpoint internally.

---

### Hugging Face Inference Providers

```toml
[llm]
provider = "huggingface"
model = "meta-llama/Llama-3.1-8B-Instruct"
base_url = ""  # leave empty to use https://router.huggingface.co/v1

[llm.openai_compatible]
api_key_env = "HF_API_KEY"
```

```bash
# .env
HF_API_KEY=hf_...
```

---

### Arbitrary OpenAI-Compatible Endpoint

For self-hosted models via vLLM, LM Studio, llama.cpp server, Anyscale, Together AI, Fireworks, etc.

```toml
[llm]
provider = "openai_compatible"
model = "your-model-name"
base_url = "https://your-endpoint.example.com/v1"

[llm.openai_compatible]
api_key_env = "YOUR_API_KEY_ENV_VAR"  # or omit if the endpoint requires no auth

# Optional extra headers
[llm.openai_compatible.headers]
"X-Custom-Header" = "value"
```

This uses `POST /chat/completions` — compatible with any server that implements the OpenAI chat completions API shape.

---

## Embedding Providers

Set `embedding.provider` in `config.toml`.

### fastembed-rs (default — fully local)

```toml
[embedding]
provider = "fastembed"
model = "BAAI/bge-small-en-v1.5"
```

No API key. Model is downloaded on first run to a local cache directory.

### OpenAI Embeddings

```toml
[embedding]
provider = "openai"
model = "text-embedding-3-small"
```

```bash
# .env
OPENAI_API_KEY=sk-...
```

---

## Provider Selection Guide

| Scenario | Recommended LLM | Recommended Embedding |
|----------|-----------------|----------------------|
| Fully local / air-gapped | Ollama (`llama3`) | fastembed-rs (`bge-small-en-v1.5`) |
| Best quality, cloud OK | OpenAI (`gpt-4o-mini`) | OpenAI (`text-embedding-3-small`) |
| Cost-optimized cloud | OpenRouter (`mistral-7b-instruct`) | fastembed-rs (local) |
| Latest Anthropic models | Anthropic (`claude-3-5-haiku`) | OpenAI embeddings |
| Custom / self-hosted | `openai_compatible` + your base_url | fastembed-rs (local) |

> **Note:** Embedding model dimension must remain consistent within a workspace. If you change the embedding model, you must re-index all memories (`POST /v1/workspaces/{id}/reindex`).
