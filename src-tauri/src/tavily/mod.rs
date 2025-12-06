//! Tavily web search integration
//!
//! Provides web search capabilities for the AI agent using Tavily's search API.
//! Supports configuration via settings file with environment variable fallback.

use anyhow::Result;
use parking_lot::RwLock;
use std::sync::Arc;

use crate::settings::{get_with_env_fallback, SettingsManager};

/// Manages the Tavily API key state
pub struct TavilyState {
    /// The API key (None if not configured)
    api_key: RwLock<Option<String>>,
}

impl TavilyState {
    /// Create a new TavilyState, checking for TAVILY_API_KEY from environment.
    /// This is the legacy constructor for backward compatibility.
    pub fn new() -> Self {
        let api_key = std::env::var("TAVILY_API_KEY")
            .ok()
            .filter(|k| !k.is_empty());

        if api_key.is_some() {
            tracing::info!("Tavily API key found, web search tools available");
        } else {
            tracing::debug!("TAVILY_API_KEY not set, web search tools will be unavailable");
        }

        Self {
            api_key: RwLock::new(api_key),
        }
    }

    /// Create TavilyState with settings-based configuration.
    ///
    /// Priority: settings.api_keys.tavily > $TAVILY_API_KEY
    #[allow(dead_code)]
    pub async fn with_settings(settings_manager: Arc<SettingsManager>) -> Self {
        let settings = settings_manager.get().await;

        let api_key = get_with_env_fallback(&settings.api_keys.tavily, &["TAVILY_API_KEY"], None);

        if api_key.is_some() {
            tracing::info!("Tavily API key found, web search tools available");
        } else {
            tracing::debug!("Tavily API key not configured, web search tools unavailable");
        }

        Self {
            api_key: RwLock::new(api_key),
        }
    }

    /// Check if Tavily is available (API key is set)
    pub fn is_available(&self) -> bool {
        self.api_key.read().is_some()
    }

    /// Get the API key
    fn get_api_key(&self) -> Result<String> {
        self.api_key
            .read()
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Tavily API key not configured"))
    }

    /// Perform a web search
    pub async fn search(&self, query: &str, max_results: Option<usize>) -> Result<SearchResults> {
        let api_key = self.get_api_key()?;

        let request = tavily::SearchRequest {
            api_key,
            query: query.to_string(),
            search_depth: Some("basic".to_string()),
            include_answer: Some(true),
            include_images: Some(false),
            include_raw_content: Some(false),
            max_results: max_results.map(|n| n as i32),
            include_domains: None,
            exclude_domains: None,
        };

        let response = tavily::search(request)
            .await
            .map_err(|e| anyhow::anyhow!("Search failed: {}", e))?;

        Ok(SearchResults {
            query: response.query,
            results: response
                .results
                .into_iter()
                .map(|r| SearchResult {
                    title: r.title,
                    url: r.url,
                    content: r.content,
                    score: r.score as f64,
                })
                .collect(),
            answer: response.answer,
        })
    }

    /// Get an AI-generated answer for a query (search with answer included)
    pub async fn answer(&self, query: &str) -> Result<AnswerResult> {
        let api_key = self.get_api_key()?;

        let request = tavily::SearchRequest {
            api_key,
            query: query.to_string(),
            search_depth: Some("advanced".to_string()),
            include_answer: Some(true),
            include_images: Some(false),
            include_raw_content: Some(false),
            max_results: Some(5),
            include_domains: None,
            exclude_domains: None,
        };

        let response = tavily::search(request)
            .await
            .map_err(|e| anyhow::anyhow!("Answer search failed: {}", e))?;

        Ok(AnswerResult {
            query: response.query,
            answer: response.answer.unwrap_or_default(),
            sources: response
                .results
                .into_iter()
                .take(5)
                .map(|r| SearchResult {
                    title: r.title,
                    url: r.url,
                    content: r.content,
                    score: r.score as f64,
                })
                .collect(),
        })
    }

    /// Extract content from URLs (search with raw content included)
    /// Note: tavily v0.2 doesn't have a dedicated extract API, so we use search with raw_content
    pub async fn extract(&self, urls: Vec<String>) -> Result<ExtractResults> {
        let api_key = self.get_api_key()?;

        // For each URL, we'll search for its content
        let mut results = Vec::new();
        let mut failed_urls = Vec::new();

        for url in urls {
            let request = tavily::SearchRequest {
                api_key: api_key.clone(),
                query: format!("site:{}", url),
                search_depth: Some("advanced".to_string()),
                include_answer: Some(false),
                include_images: Some(false),
                include_raw_content: Some(true),
                max_results: Some(1),
                include_domains: Some(vec![url.clone()]),
                exclude_domains: None,
            };

            match tavily::search(request).await {
                Ok(response) => {
                    if let Some(result) = response.results.into_iter().next() {
                        results.push(ExtractResult {
                            url: result.url,
                            raw_content: result.raw_content.unwrap_or(result.content),
                        });
                    } else {
                        failed_urls.push(url);
                    }
                }
                Err(_) => {
                    failed_urls.push(url);
                }
            }
        }

        Ok(ExtractResults {
            results,
            failed_urls,
        })
    }
}

impl Default for TavilyState {
    fn default() -> Self {
        Self::new()
    }
}

/// A single search result
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub content: String,
    pub score: f64,
}

/// Search results container
#[derive(Debug)]
pub struct SearchResults {
    pub query: String,
    pub results: Vec<SearchResult>,
    pub answer: Option<String>,
}

/// Answer result with sources
#[derive(Debug)]
pub struct AnswerResult {
    pub query: String,
    pub answer: String,
    pub sources: Vec<SearchResult>,
}

/// A single extracted URL result
#[derive(Debug, Clone)]
pub struct ExtractResult {
    pub url: String,
    pub raw_content: String,
}

/// Extract results container
#[derive(Debug)]
pub struct ExtractResults {
    pub results: Vec<ExtractResult>,
    pub failed_urls: Vec<String>,
}
