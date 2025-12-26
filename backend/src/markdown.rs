//! Server-side markdown rendering with syntax highlighting.
//!
//! Uses comrak for CommonMark parsing and syntect for code highlighting.
//! Results are cached to avoid re-rendering the same content.

use std::collections::HashMap;
use std::sync::Arc;

use comrak::{markdown_to_html_with_plugins, Options, Plugins};
use comrak::plugins::syntect::SyntectAdapter;
use once_cell::sync::Lazy;
use tokio::sync::RwLock;

// Syntect adapter for code highlighting - initialized once
static SYNTECT_ADAPTER: Lazy<SyntectAdapter> = Lazy::new(|| {
    SyntectAdapter::new(Some("base16-ocean.dark"))
});

// Simple LRU-ish cache for rendered markdown
static RENDER_CACHE: Lazy<Arc<RwLock<MarkdownCache>>> = Lazy::new(|| {
    Arc::new(RwLock::new(MarkdownCache::new(500))) // Cache up to 500 entries
});

struct MarkdownCache {
    entries: HashMap<u64, (String, std::time::Instant)>,
    max_entries: usize,
}

impl MarkdownCache {
    fn new(max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            max_entries,
        }
    }

    fn get(&self, hash: u64) -> Option<String> {
        self.entries.get(&hash).map(|(html, _)| html.clone())
    }

    fn insert(&mut self, hash: u64, html: String) {
        // Prune if needed
        if self.entries.len() >= self.max_entries {
            // Remove oldest entries
            let mut entries: Vec<_> = self.entries.iter()
                .map(|(k, (_, t))| (*k, *t))
                .collect();
            entries.sort_by(|a, b| a.1.cmp(&b.1));
            
            for (key, _) in entries.into_iter().take(self.max_entries / 4) {
                self.entries.remove(&key);
            }
        }
        
        self.entries.insert(hash, (html, std::time::Instant::now()));
    }
}

/// Simple hash function for cache keys
fn hash_content(content: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

/// Render markdown to HTML with syntax highlighting.
/// 
/// This is cached - repeated calls with the same content will return cached HTML.
pub async fn render_markdown(content: &str) -> String {
    let hash = hash_content(content);
    
    // Check cache
    {
        let cache = RENDER_CACHE.read().await;
        if let Some(html) = cache.get(hash) {
            return html;
        }
    }
    
    // Render in blocking task (comrak/syntect are not async)
    let content_owned = content.to_string();
    let content_for_error = content.to_string();
    let html = tokio::task::spawn_blocking(move || {
        render_markdown_sync(&content_owned)
    })
    .await
    .unwrap_or_else(|_| format!("<pre>{}</pre>", html_escape(&content_for_error)));
    
    // Cache result
    {
        let mut cache = RENDER_CACHE.write().await;
        cache.insert(hash, html.clone());
    }
    
    html
}

/// Synchronous markdown rendering (for use in spawn_blocking)
fn render_markdown_sync(content: &str) -> String {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.render.unsafe_ = false; // Don't allow raw HTML
    options.render.escape = true;
    
    let mut plugins = Plugins::default();
    plugins.render.codefence_syntax_highlighter = Some(&*SYNTECT_ADAPTER);
    
    markdown_to_html_with_plugins(content, &options, &plugins)
}

/// Escape HTML entities
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Render multiple markdown strings in parallel.
/// Returns a vector of HTML strings in the same order.
pub async fn render_markdown_batch(contents: Vec<String>) -> Vec<String> {
    let futures: Vec<_> = contents.into_iter()
        .map(|c| async move { render_markdown(&c).await })
        .collect();
    
    futures::future::join_all(futures).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_markdown() {
        let html = render_markdown("# Hello\n\nWorld").await;
        assert!(html.contains("<h1>"));
        assert!(html.contains("Hello"));
    }

    #[tokio::test]
    async fn test_code_block() {
        let html = render_markdown("```rust\nfn main() {}\n```").await;
        assert!(html.contains("fn"));
        // Should have syntax highlighting (contains style or class attributes)
        assert!(html.contains("<pre") || html.contains("<code"));
    }

    #[tokio::test]
    async fn test_cache() {
        let content = "# Cached content";
        let html1 = render_markdown(content).await;
        let html2 = render_markdown(content).await;
        assert_eq!(html1, html2);
    }
}
