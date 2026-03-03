//! AGENTS.md generation for shared workspace roots.
//!
//! Generates an AGENTS.md document for the workspace root that Pi auto-loads
//! (Pi walks parent directories for AGENTS.md). This gives the agent context
//! about team members and shared workspace conventions.

use super::models::SharedWorkspaceMemberInfo;

/// Generate AGENTS.md content for a shared workspace root.
///
/// Pi automatically loads AGENTS.md from parent directories when working in a
/// workdir. Since each workdir is a subdirectory of the workspace root, placing
/// AGENTS.md there ensures the agent always knows the team context.
pub fn generate_users_md(workspace_name: &str, members: &[SharedWorkspaceMemberInfo]) -> String {
    let mut md = String::new();

    md.push_str(&format!("# {} - Shared Workspace\n\n", workspace_name));
    md.push_str("This is a shared workspace. Multiple users collaborate on projects here.\n");
    md.push_str("Messages are prefixed with the sender's name in square brackets, e.g. `[Alice] hello`.\n\n");

    md.push_str("## Team\n\n");
    md.push_str("| Name | Role |\n");
    md.push_str("|------|------|\n");

    for member in members {
        md.push_str(&format!(
            "| {} | {} |\n",
            member.display_name, member.role
        ));
    }

    md.push_str("\n## Conventions\n\n");
    md.push_str("- Address users by name when responding to their specific requests.\n");
    md.push_str("- All members can see the full conversation history.\n");
    md.push_str("- If users give conflicting instructions, ask for clarification.\n");

    md
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_workspace::models::MemberRole;

    #[test]
    fn test_generate_users_md() {
        let members = vec![
            SharedWorkspaceMemberInfo {
                user_id: "u1".to_string(),
                username: "alice".to_string(),
                display_name: "Alice Smith".to_string(),
                avatar_url: None,
                role: MemberRole::Owner,
                added_at: "2026-01-01".to_string(),
            },
            SharedWorkspaceMemberInfo {
                user_id: "u2".to_string(),
                username: "bob".to_string(),
                display_name: "Bob Jones".to_string(),
                avatar_url: None,
                role: MemberRole::Member,
                added_at: "2026-01-02".to_string(),
            },
        ];

        let md = generate_users_md("Team Alpha", &members);

        assert!(md.contains("# Team Alpha - Shared Workspace"));
        assert!(md.contains("| Alice Smith | owner |"));
        assert!(md.contains("| Bob Jones | member |"));
        assert!(md.contains("conflicting instructions"));
    }
}
