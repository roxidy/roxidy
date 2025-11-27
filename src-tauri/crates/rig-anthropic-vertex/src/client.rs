//! Client for Anthropic models on Google Cloud Vertex AI.

use std::path::Path;
use std::sync::Arc;

use gcp_auth::{CustomServiceAccount, TokenProvider};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};

use crate::completion::CompletionModel;
use crate::error::AnthropicVertexError;

/// Vertex AI endpoint URL template
const VERTEX_AI_ENDPOINT: &str =
    "https://{location}-aiplatform.googleapis.com/v1/projects/{project}/locations/{location}/publishers/anthropic/models/{model}";

/// OAuth2 scope for Vertex AI
const VERTEX_AI_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";

/// Token provider variants
#[derive(Clone)]
enum TokenProviderVariant {
    /// Custom service account credentials
    ServiceAccount(Arc<CustomServiceAccount>),
    /// Application default credentials (returns Arc<dyn TokenProvider>)
    Default(Arc<dyn TokenProvider>),
}

impl TokenProviderVariant {
    async fn token(&self, scopes: &[&str]) -> Result<std::sync::Arc<gcp_auth::Token>, gcp_auth::Error> {
        match self {
            TokenProviderVariant::ServiceAccount(sa) => sa.token(scopes).await,
            TokenProviderVariant::Default(provider) => provider.token(scopes).await,
        }
    }
}

/// Client for Anthropic models on Google Cloud Vertex AI.
///
/// Handles authentication with Google Cloud and provides access to Claude models.
#[derive(Clone)]
pub struct Client {
    /// HTTP client for making requests
    http_client: reqwest::Client,
    /// Google Cloud project ID
    project_id: String,
    /// Vertex AI location (e.g., "us-east5")
    location: String,
    /// Token provider for authentication
    token_provider: TokenProviderVariant,
}

impl Client {
    /// Create a new client from a service account JSON file.
    ///
    /// # Arguments
    /// * `credentials_path` - Path to the service account JSON file
    /// * `project_id` - Google Cloud project ID
    /// * `location` - Vertex AI location (e.g., "us-east5", "europe-west1")
    ///
    /// # Example
    /// ```rust,no_run
    /// use rig_anthropic_vertex::Client;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = Client::from_service_account(
    ///     "/path/to/service-account.json",
    ///     "my-project",
    ///     "us-east5",
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn from_service_account(
        credentials_path: impl AsRef<Path>,
        project_id: impl Into<String>,
        location: impl Into<String>,
    ) -> Result<Self, AnthropicVertexError> {
        let path = credentials_path.as_ref();

        let service_account = CustomServiceAccount::from_file(path).map_err(|e| {
            AnthropicVertexError::CredentialsError {
                path: path.display().to_string(),
                message: e.to_string(),
            }
        })?;

        let http_client = reqwest::Client::builder()
            .build()
            .map_err(AnthropicVertexError::HttpError)?;

        Ok(Self {
            http_client,
            project_id: project_id.into(),
            location: location.into(),
            token_provider: TokenProviderVariant::ServiceAccount(Arc::new(service_account)),
        })
    }

    /// Create a new client from a service account JSON string.
    ///
    /// # Arguments
    /// * `credentials_json` - Service account credentials as a JSON string
    /// * `project_id` - Google Cloud project ID
    /// * `location` - Vertex AI location
    pub async fn from_service_account_json(
        credentials_json: &str,
        project_id: impl Into<String>,
        location: impl Into<String>,
    ) -> Result<Self, AnthropicVertexError> {
        let service_account =
            CustomServiceAccount::from_json(credentials_json).map_err(|e| {
                AnthropicVertexError::AuthenticationError(format!(
                    "Failed to parse credentials JSON: {}",
                    e
                ))
            })?;

        let http_client = reqwest::Client::builder()
            .build()
            .map_err(AnthropicVertexError::HttpError)?;

        Ok(Self {
            http_client,
            project_id: project_id.into(),
            location: location.into(),
            token_provider: TokenProviderVariant::ServiceAccount(Arc::new(service_account)),
        })
    }

    /// Create a new client using Application Default Credentials.
    ///
    /// This uses the `GOOGLE_APPLICATION_CREDENTIALS` environment variable
    /// or the default gcloud credentials.
    pub async fn from_env(
        project_id: impl Into<String>,
        location: impl Into<String>,
    ) -> Result<Self, AnthropicVertexError> {
        let auth_manager = gcp_auth::provider()
            .await
            .map_err(|e| AnthropicVertexError::AuthenticationError(e.to_string()))?;

        let http_client = reqwest::Client::builder()
            .build()
            .map_err(AnthropicVertexError::HttpError)?;

        Ok(Self {
            http_client,
            project_id: project_id.into(),
            location: location.into(),
            token_provider: TokenProviderVariant::Default(auth_manager),
        })
    }

    /// Get a completion model for the specified model ID.
    ///
    /// # Arguments
    /// * `model` - Model identifier (e.g., "claude-opus-4-5@20251101")
    ///
    /// # Example
    /// ```rust,no_run
    /// use rig_anthropic_vertex::{Client, models};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = Client::from_env("project", "location").await?;
    /// let model = client.completion_model(models::CLAUDE_OPUS_4_5);
    /// # Ok(())
    /// # }
    /// ```
    pub fn completion_model(&self, model: &str) -> CompletionModel {
        CompletionModel::new(self.clone(), model.to_string())
    }

    /// Build the endpoint URL for a given model and operation.
    pub(crate) fn endpoint_url(&self, model: &str, operation: &str) -> String {
        let base = VERTEX_AI_ENDPOINT
            .replace("{location}", &self.location)
            .replace("{project}", &self.project_id)
            .replace("{model}", model);
        format!("{}:{}", base, operation)
    }

    /// Get an access token for authentication.
    pub(crate) async fn get_token(&self) -> Result<String, AnthropicVertexError> {
        let token = self
            .token_provider
            .token(&[VERTEX_AI_SCOPE])
            .await
            .map_err(|e| AnthropicVertexError::TokenRefreshError(e.to_string()))?;

        Ok(token.as_str().to_string())
    }

    /// Build headers with authentication.
    pub(crate) async fn build_headers(&self) -> Result<HeaderMap, AnthropicVertexError> {
        let token = self.get_token().await?;

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token))
                .map_err(|e| AnthropicVertexError::ConfigError(e.to_string()))?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        Ok(headers)
    }

    /// Get the HTTP client.
    pub(crate) fn http_client(&self) -> &reqwest::Client {
        &self.http_client
    }

    /// Get the project ID.
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// Get the location.
    pub fn location(&self) -> &str {
        &self.location
    }
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("project_id", &self.project_id)
            .field("location", &self.location)
            .finish_non_exhaustive()
    }
}
