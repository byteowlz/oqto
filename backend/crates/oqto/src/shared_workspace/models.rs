//! Shared workspace data models.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use ts_rs::TS;

/// Member role in a shared workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum MemberRole {
    Owner,
    Admin,
    Member,
    Viewer,
}

impl std::fmt::Display for MemberRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemberRole::Owner => write!(f, "owner"),
            MemberRole::Admin => write!(f, "admin"),
            MemberRole::Member => write!(f, "member"),
            MemberRole::Viewer => write!(f, "viewer"),
        }
    }
}

impl std::str::FromStr for MemberRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "owner" => Ok(MemberRole::Owner),
            "admin" => Ok(MemberRole::Admin),
            "member" => Ok(MemberRole::Member),
            "viewer" => Ok(MemberRole::Viewer),
            _ => Err(format!("Invalid member role: {}", s)),
        }
    }
}

impl sqlx::Type<sqlx::Sqlite> for MemberRole {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <String as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Sqlite> for MemberRole {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Sqlite as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let s = self.to_string();
        <String as sqlx::Encode<sqlx::Sqlite>>::encode(s, buf)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for MemberRole {
    fn decode(
        value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <String as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        s.parse().map_err(|e: String| e.into())
    }
}

impl TryFrom<String> for MemberRole {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl MemberRole {
    /// Whether this role can manage members (add/remove/change roles).
    pub fn can_manage_members(&self) -> bool {
        matches!(self, MemberRole::Owner | MemberRole::Admin)
    }

    /// Whether this role can create/delete workdirs and run sessions.
    pub fn can_write(&self) -> bool {
        matches!(
            self,
            MemberRole::Owner | MemberRole::Admin | MemberRole::Member
        )
    }

    /// Whether this role can send prompts to sessions.
    pub fn can_prompt(&self) -> bool {
        matches!(
            self,
            MemberRole::Owner | MemberRole::Admin | MemberRole::Member
        )
    }

    /// Whether this role can delete the workspace.
    pub fn can_delete_workspace(&self) -> bool {
        matches!(self, MemberRole::Owner)
    }
}

/// Available icons for shared workspaces (Lucide icon names).
pub const WORKSPACE_ICONS: &[&str] = &[
    "users",
    "rocket",
    "globe",
    "code",
    "building",
    "shield",
    "zap",
    "layers",
    "hexagon",
    "terminal",
    "flask-conical",
    "palette",
    "brain",
    "database",
    "network",
    "git-branch",
];

/// Curated color palette for shared workspaces.
/// Muted, desaturated tones that complement the dark green-tinted theme.
pub const WORKSPACE_COLORS: &[&str] = &[
    "#3ba77c", // primary green (matches theme primary)
    "#5b8a72", // sage
    "#7c9a92", // eucalyptus
    "#6b8f9c", // slate teal
    "#5c7d8a", // steel blue
    "#7b8fa6", // dusty blue
    "#8b7fa3", // muted violet
    "#9c7b8f", // mauve
    "#a67c7c", // dusty rose
    "#b0926b", // warm sand
    "#8a9670", // olive
    "#6b9080", // seafoam
];

/// Shared workspace database row.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SharedWorkspace {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub linux_user: String,
    pub path: String,
    pub owner_id: String,
    pub description: Option<String>,
    /// Lucide icon name.
    pub icon: String,
    /// Hex color for accent.
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Shared workspace member database row.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SharedWorkspaceMember {
    pub shared_workspace_id: String,
    pub user_id: String,
    #[sqlx(try_from = "String")]
    pub role: MemberRole,
    pub added_at: String,
    pub added_by: Option<String>,
}

/// Public shared workspace info (returned to clients).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct SharedWorkspaceInfo {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub path: String,
    pub owner_id: String,
    pub description: Option<String>,
    /// Lucide icon name.
    pub icon: String,
    /// Hex color for accent.
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
    /// The requesting user's role in this workspace.
    #[sqlx(try_from = "String")]
    pub my_role: MemberRole,
    /// Number of members.
    pub member_count: i64,
}

/// Public member info (returned to clients).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct SharedWorkspaceMemberInfo {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    #[sqlx(try_from = "String")]
    pub role: MemberRole,
    pub added_at: String,
}

/// Request to create a shared workspace.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct CreateSharedWorkspaceRequest {
    /// Human-readable name.
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
    /// Lucide icon name (defaults to auto-assigned based on slug hash).
    #[serde(default)]
    pub icon: Option<String>,
    /// Hex color for accent (defaults to auto-assigned from palette).
    #[serde(default)]
    pub color: Option<String>,
    /// Initial member user IDs (the creator is added as owner automatically).
    #[serde(default)]
    pub member_ids: Vec<String>,
}

/// Request to update a shared workspace.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct UpdateSharedWorkspaceRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

/// Request to add a member to a shared workspace.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AddMemberRequest {
    pub user_id: String,
    #[serde(default = "default_member_role")]
    pub role: MemberRole,
}

fn default_member_role() -> MemberRole {
    MemberRole::Member
}

/// Request to update a member's role.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct UpdateMemberRequest {
    pub role: MemberRole,
}
