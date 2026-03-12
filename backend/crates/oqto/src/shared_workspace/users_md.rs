//! USERS.md generation for shared workspace workdirs.
//!
//! Generates a USERS.md document placed in each workdir so the agent knows
//! team members. Loaded by the custom-context-files Pi extension via
//! .pi/context.json in each workdir.

use super::models::SharedWorkspaceMemberInfo;

/// Generate USERS.md content for a shared workspace workdir.
pub fn generate_users_md(workspace_name: &str, members: &[SharedWorkspaceMemberInfo]) -> String {
    let mut md = String::new();

    md.push_str(&format!("# {} - Multi-User Chat\n\n", workspace_name));

    // Strong, unambiguous multi-user protocol description
    md.push_str("## IMPORTANT: This is a multi-user conversation\n\n");
    md.push_str(
        "Multiple people send messages in this chat. \
         Each user message starts with a sender tag on its own line:\n\n",
    );
    md.push_str("```\n");
    md.push_str("@sender: Alice\n");
    md.push_str("Can you review the script?\n");
    md.push_str("```\n\n");
    md.push_str(
        "The `@sender:` line is injected automatically by the system. \
         It is NOT typed by the user. Treat it as metadata identifying who is speaking.\n\n",
    );

    md.push_str("## Team members\n\n");
    md.push_str("| Name | Role |\n");
    md.push_str("|------|------|\n");
    for member in members {
        md.push_str(&format!("| {} | {} |\n", member.display_name, member.role));
    }

    md.push_str("\n## Rules\n\n");
    md.push_str(
        "1. When you see `@sender: Name`, that tells you WHO sent the message that follows.\n",
    );
    md.push_str("2. Address users by their name. Say \"Hi Wismut\" not \"Hi there\".\n");
    md.push_str("3. NEVER write `@sender:` in your own responses. Only the system adds that.\n");
    md.push_str("4. Different people may ask different things. Keep track of who asked what.\n");
    md.push_str(
        "5. If two users give conflicting instructions, name both and ask for clarification.\n",
    );
    md.push_str("6. All team members see the full conversation.\n");

    md
}

/// Generate .pi/context.json content that tells the custom-context-files
/// extension to load USERS.md.
pub fn generate_context_json() -> String {
    serde_json::json!({
        "contextFiles": [
            {
                "names": ["USERS.md"],
                "optional": false
            }
        ]
    })
    .to_string()
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

        assert!(md.contains("# Team Alpha - Multi-User Chat"));
        assert!(md.contains("| Alice Smith | owner |"));
        assert!(md.contains("| Bob Jones | member |"));
        assert!(md.contains("@sender:"));
        assert!(md.contains("conflicting instructions"));
    }

    #[test]
    fn test_generate_context_json() {
        let json = generate_context_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["contextFiles"][0]["names"][0], "USERS.md");
        assert_eq!(parsed["contextFiles"][0]["optional"], false);
    }
}
