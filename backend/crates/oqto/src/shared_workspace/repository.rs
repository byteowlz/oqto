//! Shared workspace repository for database operations.

use anyhow::{Context, Result, bail};
use sqlx::SqlitePool;
use tracing::{debug, instrument};

use super::models::{
    AdminSharedWorkspaceInfo, MemberRole, SharedWorkspace, SharedWorkspaceInfo,
    SharedWorkspaceMember, SharedWorkspaceMemberInfo, WORKSPACE_COLORS, WORKSPACE_ICONS,
};

/// Repository for shared workspace database operations.
#[derive(Debug, Clone)]
pub struct SharedWorkspaceRepository {
    pool: SqlitePool,
}

impl SharedWorkspaceRepository {
    /// Create a new repository.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Generate a slug from a workspace name.
    pub fn slugify(name: &str) -> String {
        let slug: String = name
            .trim()
            .to_lowercase()
            .chars()
            .map(|c| match c {
                'a'..='z' | '0'..='9' => c,
                ' ' | '_' | '.' => '-',
                _ => '-',
            })
            .collect();
        // Collapse multiple hyphens and trim
        let mut result = String::new();
        let mut prev_hyphen = false;
        for c in slug.chars() {
            if c == '-' {
                if !prev_hyphen && !result.is_empty() {
                    result.push('-');
                }
                prev_hyphen = true;
            } else {
                result.push(c);
                prev_hyphen = false;
            }
        }
        let result = result.trim_end_matches('-').to_string();
        if result.is_empty() {
            "workspace".to_string()
        } else {
            result
        }
    }

    /// Pick an icon deterministically from the slug.
    pub fn auto_icon(slug: &str) -> &'static str {
        let hash = slug.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
        WORKSPACE_ICONS[hash as usize % WORKSPACE_ICONS.len()]
    }

    /// Pick a color deterministically from the slug.
    pub fn auto_color(slug: &str) -> &'static str {
        let hash = slug
            .bytes()
            .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
        WORKSPACE_COLORS[hash as usize % WORKSPACE_COLORS.len()]
    }

    /// Create a shared workspace record.
    #[instrument(skip(self), fields(name = %name, slug = %slug))]
    pub async fn create(
        &self,
        id: &str,
        name: &str,
        slug: &str,
        linux_user: &str,
        path: &str,
        owner_id: &str,
        description: Option<&str>,
        icon: &str,
        color: &str,
    ) -> Result<SharedWorkspace> {
        let row = sqlx::query_as::<sqlx::Sqlite, SharedWorkspace>(
            r#"
            INSERT INTO shared_workspaces (id, name, slug, linux_user, path, owner_id, description, icon, color)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(slug)
        .bind(linux_user)
        .bind(path)
        .bind(owner_id)
        .bind(description)
        .bind(icon)
        .bind(color)
        .fetch_one(&self.pool)
        .await
        .context("creating shared workspace")?;

        debug!("created shared workspace: id={}, slug={}", id, slug);
        Ok(row)
    }

    /// Get a shared workspace by ID.
    pub async fn get_by_id(&self, id: &str) -> Result<Option<SharedWorkspace>> {
        let row = sqlx::query_as::<sqlx::Sqlite, SharedWorkspace>(
            "SELECT * FROM shared_workspaces WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("fetching shared workspace by id")?;
        Ok(row)
    }

    /// Get a shared workspace by slug.
    pub async fn get_by_slug(&self, slug: &str) -> Result<Option<SharedWorkspace>> {
        let row = sqlx::query_as::<sqlx::Sqlite, SharedWorkspace>(
            "SELECT * FROM shared_workspaces WHERE slug = ?",
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await
        .context("fetching shared workspace by slug")?;
        Ok(row)
    }

    /// Get a shared workspace by filesystem path.
    pub async fn get_by_path(&self, path: &str) -> Result<Option<SharedWorkspace>> {
        let row = sqlx::query_as::<sqlx::Sqlite, SharedWorkspace>(
            "SELECT * FROM shared_workspaces WHERE path = ?",
        )
        .bind(path)
        .fetch_optional(&self.pool)
        .await
        .context("fetching shared workspace by path")?;
        Ok(row)
    }

    /// List all shared workspaces a user has access to, with their role and member count.
    pub async fn list_for_user(&self, user_id: &str) -> Result<Vec<SharedWorkspaceInfo>> {
        let rows = sqlx::query_as::<sqlx::Sqlite, SharedWorkspaceInfo>(
            r#"
            SELECT
                sw.id,
                sw.name,
                sw.slug,
                sw.linux_user,
                sw.path,
                sw.owner_id,
                sw.description,
                sw.icon,
                sw.color,
                sw.created_at,
                sw.updated_at,
                swm.role AS my_role,
                (SELECT COUNT(*) FROM shared_workspace_members WHERE shared_workspace_id = sw.id) AS member_count
            FROM shared_workspaces sw
            JOIN shared_workspace_members swm ON swm.shared_workspace_id = sw.id AND swm.user_id = ?
            ORDER BY sw.name
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .context("listing shared workspaces for user")?;
        Ok(rows)
    }

    /// Update a shared workspace's name/description/icon/color.
    pub async fn update(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        icon: Option<&str>,
        color: Option<&str>,
    ) -> Result<SharedWorkspace> {
        let row = sqlx::query_as::<sqlx::Sqlite, SharedWorkspace>(
            r#"
            UPDATE shared_workspaces
            SET name = COALESCE(?2, name),
                description = COALESCE(?3, description),
                icon = COALESCE(?4, icon),
                color = COALESCE(?5, color),
                updated_at = datetime('now')
            WHERE id = ?1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .bind(icon)
        .bind(color)
        .fetch_one(&self.pool)
        .await
        .context("updating shared workspace")?;

        Ok(row)
    }

    /// Delete a shared workspace (cascades to members).
    pub async fn delete(&self, id: &str) -> Result<()> {
        let result = sqlx::query("DELETE FROM shared_workspaces WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("deleting shared workspace")?;

        if result.rows_affected() == 0 {
            bail!("shared workspace not found: {}", id);
        }
        Ok(())
    }

    // ========================================================================
    // Member operations
    // ========================================================================

    /// Add a member to a shared workspace.
    #[instrument(skip(self))]
    pub async fn add_member(
        &self,
        workspace_id: &str,
        user_id: &str,
        role: MemberRole,
        added_by: Option<&str>,
    ) -> Result<SharedWorkspaceMember> {
        let row = sqlx::query_as::<sqlx::Sqlite, SharedWorkspaceMember>(
            r#"
            INSERT INTO shared_workspace_members (shared_workspace_id, user_id, role, added_by)
            VALUES (?, ?, ?, ?)
            RETURNING *
            "#,
        )
        .bind(workspace_id)
        .bind(user_id)
        .bind(role)
        .bind(added_by)
        .fetch_one(&self.pool)
        .await
        .context("adding member to shared workspace")?;

        debug!(
            "added member {} to workspace {} as {}",
            user_id, workspace_id, role
        );
        Ok(row)
    }

    /// Get a member's role in a workspace.
    pub async fn get_member(
        &self,
        workspace_id: &str,
        user_id: &str,
    ) -> Result<Option<SharedWorkspaceMember>> {
        let row = sqlx::query_as::<sqlx::Sqlite, SharedWorkspaceMember>(
            r#"
            SELECT * FROM shared_workspace_members
            WHERE shared_workspace_id = ? AND user_id = ?
            "#,
        )
        .bind(workspace_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .context("fetching workspace member")?;
        Ok(row)
    }

    /// List all members of a shared workspace with user info.
    pub async fn list_members(&self, workspace_id: &str) -> Result<Vec<SharedWorkspaceMemberInfo>> {
        let rows = sqlx::query_as::<sqlx::Sqlite, SharedWorkspaceMemberInfo>(
            r#"
            SELECT
                swm.user_id,
                u.username,
                u.display_name,
                u.avatar_url,
                swm.role,
                swm.added_at
            FROM shared_workspace_members swm
            JOIN users u ON u.id = swm.user_id
            WHERE swm.shared_workspace_id = ?
            ORDER BY
                CASE swm.role
                    WHEN 'owner' THEN 0
                    WHEN 'admin' THEN 1
                    WHEN 'member' THEN 2
                    WHEN 'viewer' THEN 3
                END,
                u.display_name
            "#,
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await
        .context("listing workspace members")?;
        Ok(rows)
    }

    /// Update a member's role.
    pub async fn update_member_role(
        &self,
        workspace_id: &str,
        user_id: &str,
        role: MemberRole,
    ) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE shared_workspace_members
            SET role = ?
            WHERE shared_workspace_id = ? AND user_id = ?
            "#,
        )
        .bind(role)
        .bind(workspace_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .context("updating member role")?;

        if result.rows_affected() == 0 {
            bail!("member {} not found in workspace {}", user_id, workspace_id);
        }
        Ok(())
    }

    /// Remove a member from a shared workspace.
    pub async fn remove_member(&self, workspace_id: &str, user_id: &str) -> Result<()> {
        let result = sqlx::query(
            r#"
            DELETE FROM shared_workspace_members
            WHERE shared_workspace_id = ? AND user_id = ?
            "#,
        )
        .bind(workspace_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .context("removing member from workspace")?;

        if result.rows_affected() == 0 {
            bail!("member {} not found in workspace {}", user_id, workspace_id);
        }
        Ok(())
    }

    /// Check if a path is inside a shared workspace.
    /// Returns the shared workspace if the path matches or is a subdirectory.
    pub async fn find_workspace_for_path(&self, path: &str) -> Result<Option<SharedWorkspace>> {
        // Check if path starts with any shared workspace path
        let workspaces =
            sqlx::query_as::<sqlx::Sqlite, SharedWorkspace>("SELECT * FROM shared_workspaces")
                .fetch_all(&self.pool)
                .await
                .context("fetching all shared workspaces")?;

        for ws in workspaces {
            if path == ws.path || path.starts_with(&format!("{}/", ws.path)) {
                return Ok(Some(ws));
            }
        }
        Ok(None)
    }

    /// Transfer ownership: set new owner, demote old owner to admin.
    /// Must be done in a transaction to prevent orphaned workspaces.
    pub async fn transfer_ownership(
        &self,
        workspace_id: &str,
        old_owner_id: &str,
        new_owner_id: &str,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.context("starting transaction")?;

        // Verify new owner is a member
        let member = sqlx::query_as::<sqlx::Sqlite, SharedWorkspaceMember>(
            "SELECT * FROM shared_workspace_members WHERE shared_workspace_id = ? AND user_id = ?",
        )
        .bind(workspace_id)
        .bind(new_owner_id)
        .fetch_optional(&mut *tx)
        .await
        .context("checking new owner membership")?;

        if member.is_none() {
            bail!("new owner must be an existing member of the workspace");
        }

        // Update workspace owner_id
        sqlx::query(
            "UPDATE shared_workspaces SET owner_id = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(new_owner_id)
        .bind(workspace_id)
        .execute(&mut *tx)
        .await
        .context("updating workspace owner")?;

        // Promote new owner
        sqlx::query(
            "UPDATE shared_workspace_members SET role = 'owner' WHERE shared_workspace_id = ? AND user_id = ?",
        )
        .bind(workspace_id)
        .bind(new_owner_id)
        .execute(&mut *tx)
        .await
        .context("promoting new owner")?;

        // Demote old owner to admin
        sqlx::query(
            "UPDATE shared_workspace_members SET role = 'admin' WHERE shared_workspace_id = ? AND user_id = ?",
        )
        .bind(workspace_id)
        .bind(old_owner_id)
        .execute(&mut *tx)
        .await
        .context("demoting old owner")?;

        tx.commit().await.context("committing ownership transfer")?;
        Ok(())
    }

    // ========================================================================
    // Admin queries (no membership check -- caller must verify admin role)
    // ========================================================================

    /// List ALL shared workspaces with member counts (admin only).
    pub async fn admin_list_all(&self) -> Result<Vec<AdminSharedWorkspaceInfo>> {
        let rows = sqlx::query_as::<sqlx::Sqlite, AdminSharedWorkspaceInfo>(
            r#"
            SELECT
                sw.*,
                (SELECT COUNT(*) FROM shared_workspace_members WHERE shared_workspace_id = sw.id) AS member_count
            FROM shared_workspaces sw
            ORDER BY sw.created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("admin listing all shared workspaces")?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify() {
        assert_eq!(SharedWorkspaceRepository::slugify("My Team"), "my-team");
        assert_eq!(
            SharedWorkspaceRepository::slugify("Hello  World!!"),
            "hello-world"
        );
        assert_eq!(SharedWorkspaceRepository::slugify("  spaces  "), "spaces");
        assert_eq!(
            SharedWorkspaceRepository::slugify("alpha_beta.gamma"),
            "alpha-beta-gamma"
        );
        assert_eq!(SharedWorkspaceRepository::slugify(""), "workspace");
    }
}
