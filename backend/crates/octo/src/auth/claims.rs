//! JWT claims and user roles.

use serde::{Deserialize, Serialize};

/// User role.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Regular user.
    #[default]
    User,
    /// Administrator.
    Admin,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::User => write!(f, "user"),
            Role::Admin => write!(f, "admin"),
        }
    }
}

impl std::str::FromStr for Role {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "user" => Ok(Role::User),
            "admin" => Ok(Role::Admin),
            _ => Err(format!("unknown role: {}", s)),
        }
    }
}

/// JWT claims structure.
///
/// Supports both standard OIDC claims and custom claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID).
    pub sub: String,

    /// Issuer.
    #[serde(default)]
    pub iss: Option<String>,

    /// Audience.
    #[serde(default)]
    pub aud: Option<Vec<String>>,

    /// Expiration time (as Unix timestamp).
    pub exp: i64,

    /// Issued at (as Unix timestamp).
    #[serde(default)]
    pub iat: Option<i64>,

    /// Not before (as Unix timestamp).
    #[serde(default)]
    pub nbf: Option<i64>,

    /// JWT ID.
    #[serde(default)]
    pub jti: Option<String>,

    /// User's email.
    #[serde(default)]
    pub email: Option<String>,

    /// User's name.
    #[serde(default)]
    pub name: Option<String>,

    /// User's preferred username.
    #[serde(default)]
    pub preferred_username: Option<String>,

    /// User's roles.
    #[serde(default)]
    pub roles: Vec<String>,

    /// Custom role claim (alternative to roles array).
    #[serde(default)]
    pub role: Option<String>,
}

impl Claims {
    /// Get the effective role for the user.
    pub fn effective_role(&self) -> Role {
        // Check role field first
        if let Some(ref role) = self.role {
            if role.to_lowercase() == "admin" {
                return Role::Admin;
            }
        }

        // Check roles array
        for role in &self.roles {
            if role.to_lowercase() == "admin" {
                return Role::Admin;
            }
        }

        Role::User
    }

    /// Check if the user has admin role.
    pub fn is_admin(&self) -> bool {
        self.effective_role() == Role::Admin
    }

    /// Get the display name for the user.
    pub fn display_name(&self) -> &str {
        self.name
            .as_deref()
            .or(self.preferred_username.as_deref())
            .or(self.email.as_deref())
            .unwrap_or(&self.sub)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_display() {
        assert_eq!(Role::User.to_string(), "user");
        assert_eq!(Role::Admin.to_string(), "admin");
    }

    #[test]
    fn test_role_from_str() {
        assert_eq!("user".parse::<Role>().unwrap(), Role::User);
        assert_eq!("admin".parse::<Role>().unwrap(), Role::Admin);
        assert_eq!("Admin".parse::<Role>().unwrap(), Role::Admin);
        assert!("invalid".parse::<Role>().is_err());
    }

    #[test]
    fn test_claims_effective_role() {
        let claims = Claims {
            sub: "user1".to_string(),
            iss: None,
            aud: None,
            exp: 0,
            iat: None,
            nbf: None,
            jti: None,
            email: None,
            name: None,
            preferred_username: None,
            roles: vec![],
            role: None,
        };
        assert_eq!(claims.effective_role(), Role::User);

        let admin_claims = Claims {
            role: Some("admin".to_string()),
            ..claims.clone()
        };
        assert_eq!(admin_claims.effective_role(), Role::Admin);

        let admin_from_roles = Claims {
            roles: vec!["user".to_string(), "admin".to_string()],
            ..claims
        };
        assert_eq!(admin_from_roles.effective_role(), Role::Admin);
    }

    #[test]
    fn test_claims_display_name() {
        let claims = Claims {
            sub: "user123".to_string(),
            iss: None,
            aud: None,
            exp: 0,
            iat: None,
            nbf: None,
            jti: None,
            email: Some("user@example.com".to_string()),
            name: Some("John Doe".to_string()),
            preferred_username: Some("johnd".to_string()),
            roles: vec![],
            role: None,
        };
        assert_eq!(claims.display_name(), "John Doe");

        let claims_no_name = Claims {
            name: None,
            ..claims.clone()
        };
        assert_eq!(claims_no_name.display_name(), "johnd");

        let claims_only_email = Claims {
            name: None,
            preferred_username: None,
            ..claims.clone()
        };
        assert_eq!(claims_only_email.display_name(), "user@example.com");

        let claims_only_sub = Claims {
            name: None,
            preferred_username: None,
            email: None,
            ..claims
        };
        assert_eq!(claims_only_sub.display_name(), "user123");
    }
}
