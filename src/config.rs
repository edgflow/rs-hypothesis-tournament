//! Configuration from environment variables. In development a `.env` file is
//! loaded first (see `.env.example`); variables already set take precedence.

use anyhow::{Result, bail};

use crate::openai::DEFAULT_BASE_URL;

/// A small, current model: enough to turn explanations into bets.
pub const DEFAULT_MODEL: &str = "gpt-5.4-mini";

#[derive(Clone)]
pub struct Config {
    /// OpenAI API key. A secret: never log it.
    pub openai_api_key: String,
    /// Where the OpenAI API lives (tests point this at a local stand-in).
    pub openai_base_url: String,
    /// The model that writes the hypotheses (`HYPOTHESIS_MODEL`).
    pub model: String,
}

// Hand-written so the API key never ends up in a log.
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("openai_base_url", &self.openai_base_url)
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let var = |name: &str| {
            std::env::var(name)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let Some(openai_api_key) = var("OPENAI_API_KEY") else {
            bail!("OPENAI_API_KEY is not set (only `hypothesize` calls the model)");
        };
        Ok(Self {
            openai_api_key,
            openai_base_url: var("OPENAI_BASE_URL").unwrap_or_else(|| DEFAULT_BASE_URL.into()),
            model: var("HYPOTHESIS_MODEL").unwrap_or_else(|| DEFAULT_MODEL.into()),
        })
    }
}
