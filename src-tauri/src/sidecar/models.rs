//! Model management for the sidecar system.
//!
//! This module handles loading and managing the embedding model and LLM
//! used for semantic search and synthesis.
//!
//! Models:
//! - Embeddings: fastembed with AllMiniLM-L6-V2 (~30MB, 384 dimensions)
//! - LLM: Qwen 2.5 0.5B Instruct Q4_K_M (~400MB) via llama.cpp

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use llama_cpp_2::sampling::LlamaSampler;
#[allow(unused_imports)]
use llama_cpp_2::token::LlamaToken;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use super::storage::EMBEDDING_DIM;

/// Global LlamaBackend singleton (llama.cpp can only be initialized once per process)
static LLAMA_BACKEND: OnceLock<LlamaBackend> = OnceLock::new();

/// Default context size for the LLM
const DEFAULT_CTX_SIZE: u32 = 2048;

/// Default max tokens for generation
const DEFAULT_MAX_TOKENS: usize = 512;

/// Status of model availability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsStatus {
    /// Whether the embedding model is available
    pub embedding_available: bool,
    /// Whether the LLM model is available
    pub llm_available: bool,
    /// Size of embedding model in bytes (if available)
    pub embedding_size: Option<u64>,
    /// Size of LLM model in bytes (if available)
    pub llm_size: Option<u64>,
    /// Path to models directory
    pub models_dir: PathBuf,
    /// Whether the embedding model is loaded
    pub embedding_loaded: bool,
    /// Whether the LLM model is loaded
    pub llm_loaded: bool,
}

#[allow(dead_code)]
impl ModelsStatus {
    /// Check if all models are available
    pub fn all_available(&self) -> bool {
        self.embedding_available && self.llm_available
    }

    /// Get total model size in bytes
    pub fn total_size(&self) -> u64 {
        self.embedding_size.unwrap_or(0) + self.llm_size.unwrap_or(0)
    }

    /// Get human-readable total size
    pub fn human_total_size(&self) -> String {
        let bytes = self.total_size();
        if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    }
}

/// Download progress information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    /// Which model is being downloaded
    pub model: String,
    /// Bytes downloaded so far
    pub downloaded: u64,
    /// Total bytes to download
    pub total: u64,
    /// Percentage complete
    pub percent: f32,
    /// Whether download is complete
    pub complete: bool,
    /// Error message if any
    pub error: Option<String>,
}

impl DownloadProgress {
    /// Create a new progress update
    pub fn new(model: &str, downloaded: u64, total: u64) -> Self {
        let percent = if total > 0 {
            (downloaded as f32 / total as f32) * 100.0
        } else {
            0.0
        };

        Self {
            model: model.to_string(),
            downloaded,
            total,
            percent,
            complete: downloaded >= total && total > 0,
            error: None,
        }
    }

    /// Create a completed progress
    pub fn completed(model: &str) -> Self {
        Self {
            model: model.to_string(),
            downloaded: 0,
            total: 0,
            percent: 100.0,
            complete: true,
            error: None,
        }
    }

    /// Create an error progress
    pub fn error(model: &str, error: &str) -> Self {
        Self {
            model: model.to_string(),
            downloaded: 0,
            total: 0,
            percent: 0.0,
            complete: false,
            error: Some(error.to_string()),
        }
    }
}

/// Loaded LLM state
struct LoadedLlm {
    model: LlamaModel,
}

/// Model manager for loading and using models
pub struct ModelManager {
    /// Directory where models are stored
    models_dir: PathBuf,
    /// Embedding model (lazy loaded)
    embedding_model: Arc<RwLock<Option<TextEmbedding>>>,
    /// LLM model (lazy loaded)
    llm: Arc<RwLock<Option<LoadedLlm>>>,
}

impl ModelManager {
    /// Create a new model manager
    pub fn new(models_dir: PathBuf) -> Self {
        Self {
            models_dir,
            embedding_model: Arc::new(RwLock::new(None)),
            llm: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the status of model availability
    pub fn status(&self) -> ModelsStatus {
        let embedding_path = self.embedding_model_path();
        let llm_path = self.llm_model_path();

        // fastembed caches models in its own directory, but we check our models dir
        // for consistency with the status check
        let embedding_available = embedding_path.exists() || self.embedding_model.read().is_some();
        let llm_available = llm_path.exists();

        let embedding_size = if embedding_path.exists() {
            dir_size(&embedding_path).ok()
        } else {
            None
        };

        let llm_size = if llm_available {
            std::fs::metadata(&llm_path).ok().map(|m| m.len())
        } else {
            None
        };

        ModelsStatus {
            embedding_available,
            llm_available,
            embedding_size,
            llm_size,
            models_dir: self.models_dir.clone(),
            embedding_loaded: self.embedding_model.read().is_some(),
            llm_loaded: self.llm.read().is_some(),
        }
    }

    /// Get the path to the embedding model
    pub fn embedding_model_path(&self) -> PathBuf {
        self.models_dir.join("all-minilm-l6-v2")
    }

    /// Get the path to the LLM model
    pub fn llm_model_path(&self) -> PathBuf {
        self.models_dir.join("qwen2.5-0.5b-instruct-q4_k_m.gguf")
    }

    /// Check if the embedding model is available
    pub fn embedding_available(&self) -> bool {
        self.embedding_model_path().exists() || self.embedding_model.read().is_some()
    }

    /// Check if the LLM model is available
    pub fn llm_available(&self) -> bool {
        self.llm_model_path().exists()
    }

    /// Check if the LLM is loaded
    #[allow(dead_code)]
    pub fn llm_loaded(&self) -> bool {
        self.llm.read().is_some()
    }

    /// Ensure the models directory exists
    pub fn ensure_models_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.models_dir)?;
        Ok(())
    }

    /// Initialize the embedding model (lazy loading)
    pub fn init_embedding_model(&self) -> Result<()> {
        let mut model = self.embedding_model.write();
        if model.is_some() {
            return Ok(());
        }

        self.ensure_models_dir()?;

        tracing::info!("Initializing embedding model (AllMiniLM-L6-V2)...");

        // fastembed will download the model automatically if not cached
        let options = InitOptions::new(EmbeddingModel::AllMiniLML6V2)
            .with_cache_dir(self.models_dir.clone())
            .with_show_download_progress(true);

        let embedding =
            TextEmbedding::try_new(options).context("Failed to initialize embedding model")?;

        *model = Some(embedding);

        tracing::info!("Embedding model initialized successfully");
        Ok(())
    }

    /// Initialize the LLM model (lazy loading)
    pub fn init_llm_model(&self) -> Result<()> {
        let mut llm_guard = self.llm.write();
        if llm_guard.is_some() {
            return Ok(());
        }

        let llm_path = self.llm_model_path();
        if !llm_path.exists() {
            anyhow::bail!(
                "LLM model not found at {:?}. Please download it first.",
                llm_path
            );
        }

        tracing::info!("Initializing LLM (Qwen 2.5 0.5B)...");

        // Initialize llama.cpp backend using global singleton
        // (llama.cpp can only be initialized once per process)
        let backend = LLAMA_BACKEND
            .get_or_init(|| LlamaBackend::init().expect("Failed to initialize llama.cpp backend"));

        // Configure model parameters (no GPU by default for compatibility)
        let model_params = LlamaModelParams::default();

        // Load the model
        let model = LlamaModel::load_from_file(backend, &llm_path, &model_params)
            .context("Failed to load LLM model")?;

        *llm_guard = Some(LoadedLlm { model });

        tracing::info!("LLM initialized successfully");
        Ok(())
    }

    /// Generate embeddings for texts
    pub fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        // Initialize model if not already done
        if self.embedding_model.read().is_none() {
            self.init_embedding_model()?;
        }

        let model = self.embedding_model.read();
        let model = model.as_ref().context("Embedding model not initialized")?;

        let embeddings = model
            .embed(texts.to_vec(), None)
            .context("Failed to generate embeddings")?;

        // Verify embedding dimensions
        for emb in &embeddings {
            if emb.len() != EMBEDDING_DIM as usize {
                anyhow::bail!(
                    "Unexpected embedding dimension: {} (expected {})",
                    emb.len(),
                    EMBEDDING_DIM
                );
            }
        }

        Ok(embeddings)
    }

    /// Generate a single embedding for a text
    pub fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.embed(&[text])?;
        embeddings
            .into_iter()
            .next()
            .context("No embedding generated")
    }

    /// Generate text with LLM
    pub fn generate(&self, prompt: &str, max_tokens: usize) -> Result<String> {
        // Initialize model if not already done
        if self.llm.read().is_none() {
            self.init_llm_model()?;
        }

        let llm_guard = self.llm.read();
        let llm = llm_guard.as_ref().context("LLM not initialized")?;
        let backend = LLAMA_BACKEND.get().context("LLM backend not initialized")?;

        self.generate_with_model(backend, &llm.model, prompt, max_tokens)
    }

    /// Generate text using a specific model instance
    fn generate_with_model(
        &self,
        backend: &LlamaBackend,
        model: &LlamaModel,
        prompt: &str,
        max_tokens: usize,
    ) -> Result<String> {
        let max_tokens = if max_tokens == 0 {
            DEFAULT_MAX_TOKENS
        } else {
            max_tokens
        };

        // Create context
        let ctx_params =
            LlamaContextParams::default().with_n_ctx(NonZeroU32::new(DEFAULT_CTX_SIZE));

        let mut ctx = model
            .new_context(backend, ctx_params)
            .context("Failed to create LLM context")?;

        // Tokenize the prompt
        let tokens = model
            .str_to_token(prompt, AddBos::Always)
            .context("Failed to tokenize prompt")?;

        if tokens.is_empty() {
            return Ok(String::new());
        }

        // Create batch
        let mut batch = LlamaBatch::new(DEFAULT_CTX_SIZE as usize, 1);

        // Add prompt tokens to batch
        for (i, token) in tokens.iter().enumerate() {
            let is_last = i == tokens.len() - 1;
            batch.add(*token, i as i32, &[0], is_last)?;
        }

        // Decode the prompt
        ctx.decode(&mut batch).context("Failed to decode prompt")?;

        // Set up sampler for generation
        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(0.7),
            LlamaSampler::top_p(0.9, 1),
            LlamaSampler::dist(42),
        ]);

        // Generate tokens
        let mut output = String::new();
        let mut n_cur = batch.n_tokens();
        let n_decode = max_tokens;

        for _ in 0..n_decode {
            // Sample the next token
            let new_token = sampler.sample(&ctx, -1);

            // Check for end of generation
            if model.is_eog_token(new_token) {
                break;
            }

            // Convert token to text
            let token_str = model.token_to_str(new_token, Special::Tokenize)?;
            output.push_str(&token_str);

            // Prepare batch for next token
            batch.clear();
            batch.add(new_token, n_cur, &[0], true)?;

            // Decode
            ctx.decode(&mut batch).context("Failed to decode token")?;

            n_cur += 1;
        }

        Ok(output.trim().to_string())
    }

    /// Generate text with a chat template (for instruction-tuned models)
    #[allow(dead_code)]
    pub fn generate_chat(&self, system: &str, user: &str, max_tokens: usize) -> Result<String> {
        // Format as Qwen chat template
        let prompt = format!(
            "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
            system, user
        );

        self.generate(&prompt, max_tokens)
    }

    /// Download embedding model (fastembed handles this automatically)
    pub async fn download_embedding_model<F>(&self, progress: F) -> Result<()>
    where
        F: Fn(DownloadProgress) + Send + Sync,
    {
        tracing::info!("[sidecar-models] Starting embedding model download...");
        self.ensure_models_dir()?;

        progress(DownloadProgress::new("embedding", 0, 100));
        tracing::debug!("[sidecar-models] Initializing fastembed (will download if needed)...");

        // fastembed downloads models automatically on first use
        // This triggers the download if needed
        match self.init_embedding_model() {
            Ok(()) => {
                tracing::info!("[sidecar-models] Embedding model ready");
                progress(DownloadProgress::completed("embedding"));
                Ok(())
            }
            Err(e) => {
                tracing::error!("[sidecar-models] Embedding model download failed: {}", e);
                progress(DownloadProgress::error("embedding", &e.to_string()));
                Err(e)
            }
        }
    }

    /// Download LLM model from HuggingFace
    pub async fn download_llm_model<F>(&self, progress: F) -> Result<()>
    where
        F: Fn(DownloadProgress) + Send + Sync,
    {
        tracing::info!("[sidecar-models] Starting LLM model download...");
        self.ensure_models_dir()?;

        let llm_path = self.llm_model_path();
        if llm_path.exists() {
            tracing::info!(
                "[sidecar-models] LLM model already exists at {:?}",
                llm_path
            );
            progress(DownloadProgress::completed("llm"));
            return Ok(());
        }

        let url = "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf";

        tracing::info!("[sidecar-models] Downloading LLM from: {}", url);
        progress(DownloadProgress::new("llm", 0, 0));

        // Download with reqwest
        let client = reqwest::Client::new();
        let response = client
            .get(url)
            .send()
            .await
            .context("Failed to start LLM download")?;

        if !response.status().is_success() {
            let status = response.status();
            tracing::error!("[sidecar-models] LLM download failed: HTTP {}", status);
            progress(DownloadProgress::error("llm", &format!("HTTP {}", status)));
            anyhow::bail!("Failed to download LLM: HTTP {}", status);
        }

        let total_size = response.content_length().unwrap_or(0);
        tracing::info!(
            "[sidecar-models] LLM download started: {:.1} MB",
            total_size as f64 / (1024.0 * 1024.0)
        );
        progress(DownloadProgress::new("llm", 0, total_size));

        // Stream to file
        let mut file = tokio::fs::File::create(&llm_path)
            .await
            .context("Failed to create LLM file")?;

        let mut downloaded: u64 = 0;
        let mut last_log_percent: u8 = 0;
        let mut stream = response.bytes_stream();

        use futures::StreamExt;
        use tokio::io::AsyncWriteExt;

        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("[sidecar-models] LLM download chunk error: {}", e);
                    progress(DownloadProgress::error("llm", &e.to_string()));
                    return Err(e.into());
                }
            };

            file.write_all(&chunk)
                .await
                .context("Failed to write to LLM file")?;

            downloaded += chunk.len() as u64;
            progress(DownloadProgress::new("llm", downloaded, total_size));

            // Log progress every 10%
            let percent = if total_size > 0 {
                ((downloaded as f64 / total_size as f64) * 100.0) as u8
            } else {
                0
            };
            if percent >= last_log_percent + 10 {
                tracing::debug!(
                    "[sidecar-models] LLM download progress: {}% ({:.1} MB / {:.1} MB)",
                    percent,
                    downloaded as f64 / (1024.0 * 1024.0),
                    total_size as f64 / (1024.0 * 1024.0)
                );
                last_log_percent = percent;
            }
        }

        file.flush().await?;

        tracing::info!(
            "[sidecar-models] LLM model downloaded successfully: {:?} ({:.1} MB)",
            llm_path,
            total_size as f64 / (1024.0 * 1024.0)
        );
        progress(DownloadProgress::completed("llm"));

        Ok(())
    }
}

/// Calculate the size of a directory
fn dir_size(path: &Path) -> Result<u64> {
    let mut total = 0;

    if path.is_file() {
        return Ok(std::fs::metadata(path)?.len());
    }

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            total += std::fs::metadata(&path)?.len();
        } else if path.is_dir() {
            total += dir_size(&path)?;
        }
    }

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_models_status() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ModelManager::new(temp_dir.path().to_path_buf());

        let status = manager.status();
        assert!(!status.llm_available);
        assert!(!status.all_available());
        assert!(!status.llm_loaded);
    }

    #[test]
    fn test_download_progress() {
        let progress = DownloadProgress::new("embedding", 50, 100);
        assert_eq!(progress.percent, 50.0);
        assert!(!progress.complete);

        let progress = DownloadProgress::new("embedding", 100, 100);
        assert_eq!(progress.percent, 100.0);
        assert!(progress.complete);

        let progress = DownloadProgress::error("llm", "Network error");
        assert!(progress.error.is_some());
    }

    #[test]
    fn test_human_size() {
        let status = ModelsStatus {
            embedding_available: true,
            llm_available: true,
            embedding_size: Some(30 * 1024 * 1024),
            llm_size: Some(400 * 1024 * 1024),
            models_dir: PathBuf::from("/test"),
            embedding_loaded: false,
            llm_loaded: false,
        };

        let size = status.human_total_size();
        assert!(size.contains("MB"));
    }

    #[test]
    fn test_model_paths() {
        let manager = ModelManager::new(PathBuf::from("/models"));

        assert_eq!(
            manager.embedding_model_path(),
            PathBuf::from("/models/all-minilm-l6-v2")
        );
        assert_eq!(
            manager.llm_model_path(),
            PathBuf::from("/models/qwen2.5-0.5b-instruct-q4_k_m.gguf")
        );
    }

    // Note: This test requires network access and downloads ~30MB
    // Run with: cargo test --release test_embedding_generation -- --ignored
    #[test]
    #[ignore]
    fn test_embedding_generation() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ModelManager::new(temp_dir.path().to_path_buf());

        let texts = vec!["Hello, world!", "How are you?"];
        let embeddings = manager
            .embed(&texts.iter().map(|s| *s).collect::<Vec<_>>())
            .unwrap();

        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), EMBEDDING_DIM as usize);
        assert_eq!(embeddings[1].len(), EMBEDDING_DIM as usize);
    }

    // Note: This test requires the LLM model to be downloaded (~400MB)
    // Run with: cargo test --release test_llm_generation -- --ignored
    #[test]
    #[ignore]
    fn test_llm_generation() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ModelManager::new(temp_dir.path().to_path_buf());

        // This will fail if model isn't downloaded
        let result = manager.generate("Hello, how are you?", 50);
        assert!(result.is_err()); // Model not downloaded

        // With model downloaded, it would work
        // let output = manager.generate("What is 2+2?", 50).unwrap();
        // assert!(!output.is_empty());
    }

    #[test]
    fn test_chat_template() {
        let manager = ModelManager::new(PathBuf::from("/models"));

        // Test that generate_chat formats the prompt correctly
        // (we can't actually run it without the model)
        let _expected = "<|im_start|>system\nYou are helpful.<|im_end|>\n<|im_start|>user\nHello<|im_end|>\n<|im_start|>assistant\n";
        // The actual test would need a mock or the real model
    }
}
