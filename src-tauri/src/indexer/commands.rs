//! Tauri commands for code indexer operations

use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;
use vtcode_core::tools::tree_sitter::{
    analysis::{CodeMetrics, DependencyInfo},
    languages::SymbolInfo,
};

/// Result of indexing a file or directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexResult {
    pub files_indexed: usize,
    pub success: bool,
    pub message: String,
}

/// Search result from the indexer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexSearchResult {
    pub file_path: String,
    pub line_number: usize,
    pub line_content: String,
    pub matches: Vec<String>,
}

/// Symbol information for frontend consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolResult {
    pub name: String,
    pub kind: String,
    pub line: usize,
    pub column: usize,
    pub scope: Option<String>,
    pub signature: Option<String>,
    pub documentation: Option<String>,
}

impl From<SymbolInfo> for SymbolResult {
    fn from(info: SymbolInfo) -> Self {
        Self {
            name: info.name,
            kind: format!("{:?}", info.kind),
            line: info.position.row,
            column: info.position.column,
            scope: info.scope,
            signature: info.signature,
            documentation: info.documentation,
        }
    }
}

/// Code analysis result for frontend consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub symbols: Vec<SymbolResult>,
    pub metrics: Option<MetricsResult>,
    pub dependencies: Vec<DependencyResult>,
}

/// Code metrics result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsResult {
    pub lines_of_code: usize,
    pub lines_of_comments: usize,
    pub blank_lines: usize,
    pub functions_count: usize,
    pub classes_count: usize,
    pub variables_count: usize,
    pub imports_count: usize,
    pub comment_ratio: f64,
}

impl From<CodeMetrics> for MetricsResult {
    fn from(metrics: CodeMetrics) -> Self {
        Self {
            lines_of_code: metrics.lines_of_code,
            lines_of_comments: metrics.lines_of_comments,
            blank_lines: metrics.blank_lines,
            functions_count: metrics.functions_count,
            classes_count: metrics.classes_count,
            variables_count: metrics.variables_count,
            imports_count: metrics.imports_count,
            comment_ratio: metrics.comment_ratio,
        }
    }
}

/// Dependency information result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyResult {
    pub name: String,
    pub kind: String,
    pub source: Option<String>,
}

impl From<DependencyInfo> for DependencyResult {
    fn from(dep: DependencyInfo) -> Self {
        Self {
            name: dep.name,
            kind: format!("{:?}", dep.kind),
            source: Some(dep.source),
        }
    }
}

/// Initialize the code indexer for a workspace
#[tauri::command]
pub async fn init_indexer(
    workspace_path: String,
    state: State<'_, AppState>,
) -> Result<IndexResult, String> {
    tracing::info!("init_indexer called with workspace: {}", workspace_path);

    let path = PathBuf::from(&workspace_path);

    if !path.exists() {
        tracing::error!("Workspace path does not exist: {}", workspace_path);
        return Err(format!("Workspace path does not exist: {}", workspace_path));
    }

    tracing::debug!("Workspace path exists, initializing indexer state...");

    state.indexer_state.initialize(path).map_err(|e| {
        tracing::error!("Failed to initialize indexer: {}", e);
        e.to_string()
    })?;

    tracing::info!(
        "init_indexer completed successfully for: {}",
        workspace_path
    );

    Ok(IndexResult {
        files_indexed: 0,
        success: true,
        message: format!("Indexer initialized for workspace: {}", workspace_path),
    })
}

/// Check if the indexer is initialized
#[tauri::command]
pub fn is_indexer_initialized(state: State<'_, AppState>) -> bool {
    state.indexer_state.is_initialized()
}

/// Get the current workspace root
#[tauri::command]
pub fn get_indexer_workspace(state: State<'_, AppState>) -> Option<String> {
    state
        .indexer_state
        .workspace_root()
        .map(|p| p.to_string_lossy().to_string())
}

/// Get the count of indexed files
#[tauri::command]
pub fn get_indexed_file_count(state: State<'_, AppState>) -> Result<usize, String> {
    state
        .indexer_state
        .with_indexer(|indexer| {
            // Use all_files() instead of find_files("*") - more efficient and doesn't require regex
            Ok(indexer.all_files().len())
        })
        .map_err(|e| e.to_string())
}

/// Index a specific file
#[tauri::command]
pub async fn index_file(
    file_path: String,
    state: State<'_, AppState>,
) -> Result<IndexResult, String> {
    let path = PathBuf::from(&file_path);

    if !path.exists() {
        return Err(format!("File does not exist: {}", file_path));
    }

    state
        .indexer_state
        .with_indexer_mut(|indexer| {
            indexer.index_file(&path)?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;

    Ok(IndexResult {
        files_indexed: 1,
        success: true,
        message: format!("Indexed file: {}", file_path),
    })
}

/// Index a directory recursively
#[tauri::command]
pub async fn index_directory(
    dir_path: String,
    state: State<'_, AppState>,
) -> Result<IndexResult, String> {
    tracing::info!("index_directory called with path: {}", dir_path);

    let path = PathBuf::from(&dir_path);

    if !path.exists() {
        tracing::error!("Directory does not exist: {}", dir_path);
        return Err(format!("Directory does not exist: {}", dir_path));
    }

    tracing::debug!("Directory exists, checking indexer state...");
    tracing::debug!(
        "Indexer initialized: {}",
        state.indexer_state.is_initialized()
    );

    state
        .indexer_state
        .with_indexer_mut(|indexer| {
            tracing::info!("Starting directory indexing for: {:?}", path);
            let start = std::time::Instant::now();

            indexer.index_directory(&path)?;

            tracing::info!("Directory indexing completed in {:?}", start.elapsed(),);
            Ok(())
        })
        .map_err(|e| {
            tracing::error!("Failed to index directory: {}", e);
            e.to_string()
        })?;

    // Get the actual file count after indexing
    let files_indexed = state
        .indexer_state
        .with_indexer(|indexer| {
            let files = indexer.all_files();
            tracing::info!("Total files in index after indexing: {}", files.len());
            Ok(files.len())
        })
        .unwrap_or(0);

    tracing::info!(
        "index_directory completed successfully, {} files now in index",
        files_indexed
    );

    Ok(IndexResult {
        files_indexed,
        success: true,
        message: format!(
            "Indexed directory: {} ({} files in index)",
            dir_path, files_indexed
        ),
    })
}

/// Search for content in indexed files
#[tauri::command]
pub async fn search_code(
    pattern: String,
    path_filter: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<IndexSearchResult>, String> {
    state
        .indexer_state
        .with_indexer(|indexer| {
            let results = indexer.search(&pattern, path_filter.as_deref())?;
            Ok(results
                .into_iter()
                .map(|r| IndexSearchResult {
                    file_path: r.file_path,
                    line_number: r.line_number,
                    line_content: r.line_content,
                    matches: r.matches,
                })
                .collect())
        })
        .map_err(|e| e.to_string())
}

/// Search for files by name pattern
#[tauri::command]
pub async fn search_files(
    pattern: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    state
        .indexer_state
        .with_indexer(|indexer| {
            let results = indexer.find_files(&pattern)?;
            Ok(results)
        })
        .map_err(|e| e.to_string())
}

/// Analyze a file using tree-sitter
#[tauri::command]
pub async fn analyze_file(
    file_path: String,
    state: State<'_, AppState>,
) -> Result<AnalysisResult, String> {
    use vtcode_core::tools::tree_sitter::analysis::CodeAnalyzer;

    let path = PathBuf::from(&file_path);

    if !path.exists() {
        return Err(format!("File does not exist: {}", file_path));
    }

    // Get a fresh analyzer (TreeSitterAnalyzer is not Clone, so we create one per request)
    let mut analyzer = state
        .indexer_state
        .get_analyzer()
        .map_err(|e| e.to_string())?;

    // Parse the file
    let tree = analyzer
        .parse_file(&path)
        .await
        .map_err(|e| format!("Failed to parse file: {}", e))?;

    // Create code analyzer for the detected language and analyze
    let code_analyzer = CodeAnalyzer::new(&tree.language);
    let analysis = code_analyzer.analyze(&tree, &file_path);

    Ok(AnalysisResult {
        symbols: analysis
            .symbols
            .into_iter()
            .map(SymbolResult::from)
            .collect(),
        metrics: Some(MetricsResult::from(analysis.metrics)),
        dependencies: analysis
            .dependencies
            .into_iter()
            .map(DependencyResult::from)
            .collect(),
    })
}

/// Extract symbols from a file
#[tauri::command]
pub async fn extract_symbols(
    file_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<SymbolResult>, String> {
    use vtcode_core::tools::tree_sitter::languages::LanguageAnalyzer;

    let path = PathBuf::from(&file_path);

    if !path.exists() {
        return Err(format!("File does not exist: {}", file_path));
    }

    // Get a fresh analyzer
    let mut analyzer = state
        .indexer_state
        .get_analyzer()
        .map_err(|e| e.to_string())?;

    // Parse the file
    let tree = analyzer
        .parse_file(&path)
        .await
        .map_err(|e| format!("Failed to parse file: {}", e))?;

    // Extract symbols using language analyzer for the detected language
    let lang_analyzer = LanguageAnalyzer::new(&tree.language);
    let symbols = lang_analyzer.extract_symbols(&tree);

    Ok(symbols.into_iter().map(SymbolResult::from).collect())
}

/// Get code metrics for a file
#[tauri::command]
pub async fn get_file_metrics(
    file_path: String,
    state: State<'_, AppState>,
) -> Result<MetricsResult, String> {
    use vtcode_core::tools::tree_sitter::analysis::CodeAnalyzer;

    let path = PathBuf::from(&file_path);

    if !path.exists() {
        return Err(format!("File does not exist: {}", file_path));
    }

    // Get a fresh analyzer
    let mut analyzer = state
        .indexer_state
        .get_analyzer()
        .map_err(|e| e.to_string())?;

    // Parse the file
    let tree = analyzer
        .parse_file(&path)
        .await
        .map_err(|e| format!("Failed to parse file: {}", e))?;

    // Analyze the file and extract metrics
    let code_analyzer = CodeAnalyzer::new(&tree.language);
    let analysis = code_analyzer.analyze(&tree, &file_path);

    Ok(MetricsResult::from(analysis.metrics))
}

/// Detect the language of a file
#[tauri::command]
pub fn detect_language(file_path: String) -> Result<String, String> {
    use vtcode_core::tools::tree_sitter::analyzer::TreeSitterAnalyzer;

    let path = PathBuf::from(&file_path);

    // Create an analyzer to detect language
    let analyzer =
        TreeSitterAnalyzer::new().map_err(|e| format!("Failed to create analyzer: {}", e))?;

    match analyzer.detect_language_from_path(&path) {
        Ok(lang) => Ok(format!("{:?}", lang)),
        Err(e) => Err(format!("Could not detect language: {}", e)),
    }
}

/// Shutdown the indexer
#[tauri::command]
pub fn shutdown_indexer(state: State<'_, AppState>) {
    state.indexer_state.shutdown();
}
