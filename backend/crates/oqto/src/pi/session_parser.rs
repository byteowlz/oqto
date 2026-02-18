//! Parser for auto-generated Pi session titles.
//!
//! Pi extension auto-renames sessions to: `<workdir>: <generated_title> [adj-noun-verb]`
//! This module parses this format to:
//! - Extract readable ID (adj-noun-verb part)
//! - Strip workspace and ID for clean title display

use once_cell::sync::Lazy;
use regex::Regex;

/// Pattern for auto-generated title: `<workdir>: <generated_title> [adj-noun-verb]`
/// or `<generated_title> [adj-noun-verb]` when workspace is omitted.
/// Examples:
///   "myproject: Discuss the implementation [cold-lamp-verb]"
///   "frontend: Bug fix for login [blue-frog-fix]"
static TITLE_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"^(?:(?P<workspace>[^:]+):\s*)?(?P<title>[^\[]+)\s*\[(?P<readable_id>[^\]]+)\]\s*$"#,
    )
    .expect("Invalid regex pattern for auto-generated title")
});

/// Parsed components of an auto-generated Pi session title
#[derive(Debug, Clone)]
pub struct ParsedTitle {
    /// The workspace/project directory name
    pub workspace: Option<String>,
    /// The clean title (without workspace prefix and readable ID suffix)
    pub title: String,
    /// The readable ID (adj-noun-verb part)
    pub readable_id: Option<String>,
}

impl ParsedTitle {
    /// Parse an auto-generated Pi session title
    ///
    /// # Arguments
    /// * `title` - The full title string to parse
    ///
    /// # Returns
    /// * `ParsedTitle` with extracted components
    pub fn parse(title: &str) -> Self {
        let title = title.trim();

        // Try to match auto-generated format
        if let Some(caps) = TITLE_PATTERN.captures(title) {
            let workspace = caps
                .name("workspace")
                .map(|w| w.as_str().trim().to_string());
            let clean_title = caps
                .name("title")
                .map(|t| t.as_str().trim().to_string())
                .unwrap_or_else(|| title.to_string());
            let readable_id = caps
                .name("readable_id")
                .map(|r| r.as_str().trim().to_string());

            ParsedTitle {
                workspace,
                title: clean_title,
                readable_id,
            }
        } else {
            // Not in auto-generated format, return as-is
            ParsedTitle {
                workspace: None,
                title: title.to_string(),
                readable_id: None,
            }
        }
    }

    /// Get the display title (cleaned without workspace prefix)
    pub fn display_title(&self) -> &str {
        &self.title
    }

    /// Get the readable ID
    pub fn get_readable_id(&self) -> Option<&str> {
        self.readable_id.as_deref()
    }

    /// Get the workspace name
    pub fn get_workspace(&self) -> Option<&str> {
        self.workspace.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_auto_generated_title() {
        let parsed = ParsedTitle::parse("myproject: Discuss the implementation [cold-lamp-verb]");

        assert_eq!(parsed.workspace, Some("myproject".to_string()));
        assert_eq!(parsed.title, "Discuss the implementation");
        assert_eq!(parsed.readable_id, Some("cold-lamp-verb".to_string()));
    }

    #[test]
    fn test_parse_title_without_workspace() {
        let parsed = ParsedTitle::parse("Bug fix for login [blue-frog-fix]");

        assert_eq!(parsed.workspace, None);
        assert_eq!(parsed.title, "Bug fix for login");
        assert_eq!(parsed.readable_id, Some("blue-frog-fix".to_string()));
    }

    #[test]
    fn test_parse_plain_title() {
        let parsed = ParsedTitle::parse("Just a regular title");

        assert_eq!(parsed.workspace, None);
        assert_eq!(parsed.title, "Just a regular title");
        assert_eq!(parsed.readable_id, None);
    }

    #[test]
    fn test_parse_empty_title() {
        let parsed = ParsedTitle::parse("");

        assert_eq!(parsed.workspace, None);
        assert_eq!(parsed.title, "");
        assert_eq!(parsed.readable_id, None);
    }

    #[test]
    fn test_parse_with_extra_spaces() {
        let parsed = ParsedTitle::parse("frontend :  Discuss login flow  [  red-bird-fix  ]");

        assert_eq!(parsed.workspace, Some("frontend".to_string()));
        assert_eq!(parsed.title, "Discuss login flow");
        assert_eq!(parsed.readable_id, Some("red-bird-fix".to_string()));
    }
}
