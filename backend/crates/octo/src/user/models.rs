//! User data models.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use ts_rs::TS;

/// User role enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum UserRole {
    #[default]
    User,
    Admin,
    Service,
}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserRole::User => write!(f, "user"),
            UserRole::Admin => write!(f, "admin"),
            UserRole::Service => write!(f, "service"),
        }
    }
}

impl std::str::FromStr for UserRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "user" => Ok(UserRole::User),
            "admin" => Ok(UserRole::Admin),
            "service" => Ok(UserRole::Service),
            _ => Err(format!("Invalid role: {}", s)),
        }
    }
}

impl TryFrom<String> for UserRole {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

/// User entity from database.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: String,
    pub external_id: Option<String>,
    pub username: String,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: Option<String>,
    pub display_name: String,
    pub avatar_url: Option<String>,
    #[sqlx(try_from = "String")]
    pub role: UserRole,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
    pub settings: Option<String>,
    pub mmry_port: Option<i64>,
    pub sldr_port: Option<i64>,
    pub linux_username: Option<String>,
    /// Linux UID for multi-user isolation. Stored to verify ownership
    /// since users can modify their own GECOS via chfn.
    pub linux_uid: Option<i64>,
}

impl sqlx::Type<sqlx::Sqlite> for UserRole {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <String as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Sqlite> for UserRole {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Sqlite as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let s = self.to_string();
        <String as sqlx::Encode<sqlx::Sqlite>>::encode(s, buf)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for UserRole {
    fn decode(
        value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <String as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        s.parse().map_err(|e: String| e.into())
    }
}

/// Public user info (safe to return to clients).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub email: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub role: UserRole,
    pub is_active: bool,
    pub created_at: String,
    pub last_login_at: Option<String>,
}

impl From<User> for UserInfo {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            username: user.username,
            email: user.email,
            display_name: user.display_name,
            avatar_url: user.avatar_url,
            role: user.role,
            is_active: user.is_active,
            created_at: user.created_at,
            last_login_at: user.last_login_at,
        }
    }
}

/// Request to create a new user.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub email: String,
    pub password: Option<String>,
    pub display_name: Option<String>,
    pub role: Option<UserRole>,
    pub external_id: Option<String>,
}

/// Request to update an existing user.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdateUserRequest {
    pub username: Option<String>,
    pub email: Option<String>,
    pub password: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub role: Option<UserRole>,
    pub is_active: Option<bool>,
    pub settings: Option<String>,
    /// Linux username for multi-user isolation mode.
    /// Set this to map the Octo user to an existing Linux user.
    pub linux_username: Option<String>,
    /// Linux UID for multi-user isolation. Used to verify ownership.
    pub linux_uid: Option<i64>,
}

/// User list query parameters.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct UserListQuery {
    pub role: Option<UserRole>,
    pub is_active: Option<bool>,
    pub search: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_role_display() {
        assert_eq!(UserRole::User.to_string(), "user");
        assert_eq!(UserRole::Admin.to_string(), "admin");
        assert_eq!(UserRole::Service.to_string(), "service");
    }

    #[test]
    fn test_user_role_parse() {
        assert_eq!("user".parse::<UserRole>().unwrap(), UserRole::User);
        assert_eq!("admin".parse::<UserRole>().unwrap(), UserRole::Admin);
        assert_eq!("ADMIN".parse::<UserRole>().unwrap(), UserRole::Admin);
        assert!("invalid".parse::<UserRole>().is_err());
    }

    #[test]
    fn test_user_info_from_user() {
        let user = User {
            id: "test".to_string(),
            external_id: None,
            username: "testuser".to_string(),
            email: "test@example.com".to_string(),
            password_hash: Some("secret".to_string()),
            display_name: "Test User".to_string(),
            avatar_url: None,
            role: UserRole::User,
            is_active: true,
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-01".to_string(),
            last_login_at: None,
            settings: None,
            mmry_port: None,
            sldr_port: None,
            linux_username: None,
            linux_uid: None,
        };

        let info: UserInfo = user.into();
        assert_eq!(info.username, "testuser");
        // Password hash should not be in UserInfo
    }
}
