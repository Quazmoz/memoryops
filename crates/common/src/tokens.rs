use std::sync::LazyLock;

use anyhow::anyhow;
use tiktoken_rs::CoreBPE;

use crate::{error::AppResult, AppError};

static CL100K_TOKENIZER: LazyLock<Result<CoreBPE, String>> =
    LazyLock::new(|| tiktoken_rs::cl100k_base().map_err(|error| error.to_string()));

pub fn estimate_tokens(content: &str) -> AppResult<usize> {
    match &*CL100K_TOKENIZER {
        Ok(tokenizer) => Ok(tokenizer.encode_with_special_tokens(content).len().max(1)),
        Err(error) => Err(AppError::Internal(anyhow!(
            "failed to initialize tokenizer: {error}"
        ))),
    }
}
