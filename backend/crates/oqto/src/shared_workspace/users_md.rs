//! USERS.md generation for shared workspaces.
//!
//! Generates a markdown document listing all members of a shared workspace.
//! This file is placed at the workspace root and automatically loaded by Pi
//! as context so the agent understands it is working with multiple users.

use super::models::SharedWorkspaceMemberInfo;

/// Generate USERS.md content from a list of members.
pub fn generate_users_md(workspace_name: &str, members: &[SharedWorkspaceMemberInfo]) -> String {
    let mut md = String::new();

    md.push_str("# Team Members\n\n");
    md.push_str(&format!(
        "This is a shared workspace (**{}**). Multiple users may send messages in sessions.\n",
        workspace_name
    ));
    md.push_str("Messages are prefixed with the sender's name in square brackets.\n\n");

    md.push_str("## Members\n\n");
    md.push_str("| Name | Username | Role |\n");
    md.push_str("|------|----------|------|\n");

    for member in members {
        md.push_str(&format!(
            "| {} | {} | {} |\n",
            member.display_name, member.username, member.role
        ));
    }

    md.push_str("\n## Conventions\n\n");
    md.push_str("- Messages from users appear as: [Name] message content\n");
    md.push_str("- When addressing a specific user's request, mention their name.\n");
    md.push_str("- All members can see the full conversation history.\n");
    md.push_str("- If multiple users give conflicting instructions, ask for clarification.\n");

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

        assert!(md.contains("# Team Members"));
        assert!(md.contains("**Team Alpha**"));
        assert!(md.contains("| Alice Smith | alice | owner |"));
        assert!(md.contains("| Bob Jones | bob | member |"));
        assert!(md.contains("[Name] message content"));
    }
}
