//! Indexer state management

use parking_lot::RwLock;
use std::path::PathBuf;
use vtcode_core::tools::tree_sitter::analyzer::TreeSitterAnalyzer;
use vtcode_indexer::SimpleIndexer;

/// Load existing index entries from disk into the indexer's cache.
/// Parses Markdown files in the index directory and re-indexes files that still exist.
fn load_existing_index(indexer: &mut SimpleIndexer, index_dir: &PathBuf) -> anyhow::Result<usize> {
    let mut loaded = 0;

    if !index_dir.exists() {
        return Ok(0);
    }

    // Read all .md files in the index directory
    for entry in std::fs::read_dir(index_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        // Parse the markdown to extract the file path
        if let Ok(content) = std::fs::read_to_string(&path) {
            // Look for "- **Path**: /path/to/file" in the markdown
            for line in content.lines() {
                if let Some(file_path) = line.strip_prefix("- **Path**: ") {
                    let file_path = PathBuf::from(file_path.trim());
                    // Re-index if the file still exists
                    if file_path.exists() && indexer.index_file(&file_path).is_ok() {
                        loaded += 1;
                    }
                    break;
                }
            }
        }
    }

    Ok(loaded)
}

/// Manages the code indexer state
pub struct IndexerState {
    /// The file indexer for workspace navigation
    indexer: RwLock<Option<SimpleIndexer>>,
    /// Tree-sitter analyzer for semantic code analysis
    analyzer: RwLock<Option<TreeSitterAnalyzer>>,
    /// Current workspace root
    workspace_root: RwLock<Option<PathBuf>>,
}

impl IndexerState {
    pub fn new() -> Self {
        Self {
            indexer: RwLock::new(None),
            analyzer: RwLock::new(None),
            workspace_root: RwLock::new(None),
        }
    }

    /// Initialize the indexer for a workspace
    pub fn initialize(&self, workspace_path: PathBuf) -> anyhow::Result<()> {
        tracing::info!("Initializing indexer for workspace: {:?}", workspace_path);

        // Create the index directory inside the workspace
        let index_dir = workspace_path.join(".qbit").join("index");
        tracing::debug!("Creating index directory: {:?}", index_dir);
        std::fs::create_dir_all(&index_dir)?;
        tracing::debug!("Index directory created successfully");

        // Create the indexer with custom index directory
        tracing::debug!(
            "Creating SimpleIndexer with workspace: {:?}, index_dir: {:?}",
            workspace_path,
            index_dir
        );
        let mut indexer = SimpleIndexer::with_index_dir(workspace_path.clone(), index_dir.clone());

        // Initialize the indexer storage
        tracing::debug!("Initializing indexer storage...");
        indexer.init()?;
        tracing::debug!("Indexer storage initialized");

        // Load existing index from disk if available
        let loaded = load_existing_index(&mut indexer, &index_dir).unwrap_or(0);
        if loaded > 0 {
            tracing::info!("Loaded {} files from existing index", loaded);
        }

        // Create the tree-sitter analyzer
        tracing::debug!("Creating tree-sitter analyzer...");
        let analyzer = TreeSitterAnalyzer::new()
            .map_err(|e| anyhow::anyhow!("Failed to create tree-sitter analyzer: {}", e))?;
        tracing::debug!("Tree-sitter analyzer created");

        // Store state
        *self.indexer.write() = Some(indexer);
        *self.analyzer.write() = Some(analyzer);
        *self.workspace_root.write() = Some(workspace_path.clone());

        tracing::info!("Indexer initialized successfully for {:?}", workspace_path);
        tracing::info!("Index files will be stored in: {:?}", index_dir);
        Ok(())
    }

    /// Check if the indexer is initialized
    pub fn is_initialized(&self) -> bool {
        self.indexer.read().is_some()
    }

    /// Get the current workspace root
    pub fn workspace_root(&self) -> Option<PathBuf> {
        self.workspace_root.read().clone()
    }

    /// Access the indexer for read operations
    pub fn with_indexer<F, R>(&self, f: F) -> anyhow::Result<R>
    where
        F: FnOnce(&SimpleIndexer) -> anyhow::Result<R>,
    {
        let guard = self.indexer.read();
        match guard.as_ref() {
            Some(indexer) => f(indexer),
            None => anyhow::bail!("Indexer not initialized"),
        }
    }

    /// Access the indexer for write operations
    pub fn with_indexer_mut<F, R>(&self, f: F) -> anyhow::Result<R>
    where
        F: FnOnce(&mut SimpleIndexer) -> anyhow::Result<R>,
    {
        let mut guard = self.indexer.write();
        match guard.as_mut() {
            Some(indexer) => f(indexer),
            None => anyhow::bail!("Indexer not initialized"),
        }
    }

    /// Get a clone of the analyzer for async operations
    /// Note: TreeSitterAnalyzer is not Clone, so we need to create a new one
    /// This is acceptable because parsers are cheap to create
    pub fn get_analyzer(&self) -> anyhow::Result<TreeSitterAnalyzer> {
        if !self.is_initialized() {
            anyhow::bail!("Analyzer not initialized");
        }
        TreeSitterAnalyzer::new().map_err(|e| anyhow::anyhow!("Failed to create analyzer: {}", e))
    }

    /// Shutdown the indexer
    pub fn shutdown(&self) {
        tracing::info!("Shutting down indexer");
        *self.indexer.write() = None;
        *self.analyzer.write() = None;
        *self.workspace_root.write() = None;
    }
}

impl Default for IndexerState {
    fn default() -> Self {
        Self::new()
    }
}
