use std::path::Path;

use serde::{Deserialize, Serialize};

/// Fileserver configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Maximum file size for uploads (in bytes)
    #[serde(default = "default_max_upload_size")]
    pub max_upload_size: u64,

    /// Maximum depth for directory traversal
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,

    /// File extensions to hide in simple mode
    #[serde(default = "default_hidden_extensions")]
    pub hidden_extensions: Vec<String>,

    /// Directories to always hide
    #[serde(default = "default_hidden_dirs")]
    pub hidden_dirs: Vec<String>,

    /// Office/document extensions for simple view filtering
    #[serde(default = "default_office_extensions")]
    pub office_extensions: Vec<String>,
}

fn default_max_upload_size() -> u64 {
    100 * 1024 * 1024 // 100 MB
}

fn default_max_depth() -> usize {
    20
}

fn default_hidden_extensions() -> Vec<String> {
    vec![
        ".pyc".to_string(),
        ".pyo".to_string(),
        ".o".to_string(),
        ".so".to_string(),
        ".dylib".to_string(),
    ]
}

fn default_hidden_dirs() -> Vec<String> {
    vec![
        ".git".to_string(),
        "node_modules".to_string(),
        "__pycache__".to_string(),
        ".cache".to_string(),
        "target".to_string(),
        ".venv".to_string(),
        "venv".to_string(),
    ]
}

fn default_office_extensions() -> Vec<String> {
    vec![
        // Documents
        ".txt".to_string(),
        ".md".to_string(),
        ".pdf".to_string(),
        ".doc".to_string(),
        ".docx".to_string(),
        ".odt".to_string(),
        ".rtf".to_string(),
        // Spreadsheets
        ".csv".to_string(),
        ".xls".to_string(),
        ".xlsx".to_string(),
        ".ods".to_string(),
        // Presentations
        ".ppt".to_string(),
        ".pptx".to_string(),
        ".odp".to_string(),
        // Images
        ".png".to_string(),
        ".jpg".to_string(),
        ".jpeg".to_string(),
        ".gif".to_string(),
        ".svg".to_string(),
        ".webp".to_string(),
        // Data
        ".json".to_string(),
        ".xml".to_string(),
        ".yaml".to_string(),
        ".yml".to_string(),
    ]
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_upload_size: default_max_upload_size(),
            max_depth: default_max_depth(),
            hidden_extensions: default_hidden_extensions(),
            hidden_dirs: default_hidden_dirs(),
            office_extensions: default_office_extensions(),
        }
    }
}

impl Config {
    /// Load config from a TOML file
    pub fn from_file(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Check if an extension should be hidden
    pub fn is_hidden_extension(&self, ext: &str) -> bool {
        self.hidden_extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
    }

    /// Check if a directory should be hidden
    pub fn is_hidden_dir(&self, name: &str) -> bool {
        self.hidden_dirs.iter().any(|d| d == name)
    }

    /// Check if a file is an "office" type file (for simple view)
    pub fn is_office_file(&self, ext: &str) -> bool {
        self.office_extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
    }
}
