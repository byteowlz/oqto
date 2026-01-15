//! Authentication middleware.

use axum::{
    extract::{FromRequestParts, State},
    http::{header::AUTHORIZATION, request::Parts},
    middleware::Next,
    response::Response,
};
use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use log::{debug, warn};
use std::sync::Arc;

use super::{AuthConfig, AuthError, Claims, DevUser, Role};

/// Extract a Bearer token from an Authorization header value.
fn bearer_token_from_header(header_value: &str) -> Result<&str, AuthError> {
    let mut parts = header_value.split_whitespace();
    let scheme = parts.next().ok_or(AuthError::InvalidAuthHeader)?;

    if !scheme.eq_ignore_ascii_case("bearer") {
        return Err(AuthError::InvalidAuthHeader);
    }

    let token = parts.next().ok_or(AuthError::InvalidAuthHeader)?;
    if token.is_empty() {
        return Err(AuthError::InvalidAuthHeader);
    }

    if parts.next().is_some() {
        return Err(AuthError::InvalidAuthHeader);
    }

    Ok(token)
}

fn token_from_cookie_header<'a>(cookie_header: &'a str, cookie_name: &str) -> Option<&'a str> {
    cookie_header.split(';').map(str::trim).find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        if name.trim() == cookie_name {
            Some(value.trim())
        } else {
            None
        }
    })
}

/// Authentication state shared across handlers.
#[derive(Clone)]
pub struct AuthState {
    config: Arc<AuthConfig>,
    decoding_key: Option<DecodingKey>,
}

impl AuthState {
    /// Create new auth state from config.
    /// Resolves `env:VAR_NAME` syntax in jwt_secret at construction time.
    pub fn new(mut config: AuthConfig) -> Self {
        // Resolve jwt_secret if it uses env: syntax
        if let Ok(Some(resolved)) = config.resolve_jwt_secret() {
            config.jwt_secret = Some(resolved);
        }

        let decoding_key = config
            .jwt_secret
            .as_ref()
            .map(|s| DecodingKey::from_secret(s.as_bytes()));

        Self {
            config: Arc::new(config),
            decoding_key,
        }
    }

    /// Check if dev mode is enabled.
    pub fn is_dev_mode(&self) -> bool {
        self.config.dev_mode
    }

    /// Get dev users.
    #[allow(dead_code)]
    pub fn dev_users(&self) -> &[DevUser] {
        &self.config.dev_users
    }

    /// Get allowed CORS origins from config.
    pub fn allowed_origins(&self) -> &[String] {
        &self.config.allowed_origins
    }

    /// Validate credentials in dev mode.
    /// Uses bcrypt password verification for security.
    pub fn validate_dev_credentials(&self, username: &str, password: &str) -> Option<&DevUser> {
        if !self.config.dev_mode {
            return None;
        }

        self.config
            .dev_users
            .iter()
            .find(|u| (u.id == username || u.email == username) && u.verify_password(password))
    }

    /// Validate a JWT token.
    pub fn validate_token(&self, token: &str) -> Result<Claims, AuthError> {
        // In dev mode with no token, try to find a matching dev user header
        if self.config.dev_mode {
            // Check if this is a dev token (prefixed with "dev:")
            if let Some(user_id) = token.strip_prefix("dev:") {
                return self.get_dev_user_claims(user_id);
            }
        }

        // Validate JWT
        let decoding_key = self
            .decoding_key
            .as_ref()
            .ok_or_else(|| AuthError::Internal("no JWT secret configured".to_string()))?;

        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        validation.validate_nbf = false;
        validation.required_spec_claims.clear(); // Allow missing iss/aud

        let token_data = decode::<Claims>(token, decoding_key, &validation).map_err(|e| {
            warn!("JWT validation failed: {:?}", e);
            match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::TokenExpired,
                _ => AuthError::InvalidToken(e.to_string()),
            }
        })?;

        Ok(token_data.claims)
    }

    /// Get claims for a dev user.
    fn get_dev_user_claims(&self, user_id: &str) -> Result<Claims, AuthError> {
        let user = self
            .config
            .dev_users
            .iter()
            .find(|u| u.id == user_id)
            .ok_or(AuthError::UserNotFound)?;

        Ok(Claims {
            sub: user.id.clone(),
            iss: Some("dev".to_string()),
            aud: None,
            exp: Utc::now().timestamp() + 3600 * 24, // 24 hours
            iat: Some(Utc::now().timestamp()),
            nbf: None,
            jti: None,
            email: Some(user.email.clone()),
            name: Some(user.name.clone()),
            preferred_username: Some(user.id.clone()),
            roles: vec![user.role.to_string()],
            role: Some(user.role.to_string()),
        })
    }

    /// Generate a dev token for a user.
    pub fn generate_dev_token(&self, user: &DevUser) -> Result<String, AuthError> {
        self.generate_token(&user.id, &user.email, &user.name, &user.role.to_string())
    }

    /// Generate a JWT token for any user.
    pub fn generate_token(
        &self,
        user_id: &str,
        email: &str,
        name: &str,
        role: &str,
    ) -> Result<String, AuthError> {
        use jsonwebtoken::{EncodingKey, Header, encode};

        let secret = self
            .config
            .jwt_secret
            .as_ref()
            .ok_or_else(|| AuthError::Internal("no JWT secret configured".to_string()))?;

        let claims = Claims {
            sub: user_id.to_string(),
            iss: Some("workspace-backend".to_string()),
            aud: None,
            exp: Utc::now().timestamp() + 3600 * 24, // 24 hours
            iat: Some(Utc::now().timestamp()),
            nbf: None,
            jti: None,
            email: Some(email.to_string()),
            name: Some(name.to_string()),
            preferred_username: Some(user_id.to_string()),
            roles: vec![role.to_string()],
            role: Some(role.to_string()),
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .map_err(|e| AuthError::Internal(e.to_string()))
    }
}

/// Authenticated user extracted from request.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    /// User claims.
    pub claims: Claims,
}

impl CurrentUser {
    /// Get the user ID.
    pub fn id(&self) -> &str {
        &self.claims.sub
    }

    /// Get the user's role.
    pub fn role(&self) -> Role {
        self.claims.effective_role()
    }

    /// Check if user is admin.
    pub fn is_admin(&self) -> bool {
        self.claims.is_admin()
    }

    /// Get display name.
    pub fn display_name(&self) -> &str {
        self.claims.display_name()
    }
}

/// Extract authentication from request.
impl<S> FromRequestParts<S> for CurrentUser
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<CurrentUser>()
            .cloned()
            .ok_or(AuthError::MissingAuthHeader)
    }
}

/// Authentication middleware.
///
/// Validates JWT tokens and injects `CurrentUser` into request extensions.
/// Supports multiple auth methods in priority order:
/// 1. Authorization: Bearer <token> header
/// 2. auth_token cookie
/// 3. token query parameter (for WebSocket connections)
/// 4. X-Dev-User header (dev mode only)
pub async fn auth_middleware(
    State(auth): State<AuthState>,
    mut req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Result<Response, AuthError> {
    // Get authorization header
    let auth_header = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    // Allow cookie-based auth for browser clients (EventSource/WebSocket don't support custom headers).
    let cookie_token = req
        .headers()
        .get(axum::http::header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .and_then(|cookie_header| token_from_cookie_header(cookie_header, "auth_token"));

    // Allow token in query parameter for WebSocket connections (browsers can't set headers on WS)
    let query_token = req.uri().query().and_then(|q| {
        q.split('&').find_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?;
            let value = parts.next()?;
            if key == "token" {
                // URL decode the token value
                urlencoding::decode(value).ok().map(|s| s.into_owned())
            } else {
                None
            }
        })
    });

    let claims = if let Some(header) = auth_header {
        // Parse Bearer token
        let token = bearer_token_from_header(header)?;

        // Validate token
        auth.validate_token(token)?
    } else if let Some(token) = cookie_token {
        auth.validate_token(token)?
    } else if let Some(ref token) = query_token {
        auth.validate_token(token)?
    } else if auth.is_dev_mode() {
        // In dev mode, allow X-Dev-User header
        if let Some(user_id) = req
            .headers()
            .get("X-Dev-User")
            .and_then(|h| h.to_str().ok())
        {
            debug!("Using dev user: {}", user_id);
            auth.validate_token(&format!("dev:{}", user_id))?
        } else {
            return Err(AuthError::MissingAuthHeader);
        }
    } else {
        return Err(AuthError::MissingAuthHeader);
    };

    // Inject current user into extensions
    let user = CurrentUser { claims };
    req.extensions_mut().insert(user);

    Ok(next.run(req).await)
}

/// Require admin role.
///
/// Use as an extractor in handlers that require admin access.
#[derive(Debug, Clone)]
pub struct RequireAdmin(pub CurrentUser);

impl<S> FromRequestParts<S> for RequireAdmin
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let user = parts
            .extensions
            .get::<CurrentUser>()
            .cloned()
            .ok_or(AuthError::MissingAuthHeader)?;

        if !user.is_admin() {
            return Err(AuthError::InsufficientPermissions(
                "admin role required".to_string(),
            ));
        }

        Ok(RequireAdmin(user))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bearer_token_from_header_valid() {
        assert_eq!(
            bearer_token_from_header("Bearer abc.def.ghi").unwrap(),
            "abc.def.ghi"
        );
        assert_eq!(
            bearer_token_from_header("bearer   token123").unwrap(),
            "token123"
        );
        assert_eq!(
            bearer_token_from_header("   Bearer\tmixed-case ").unwrap(),
            "mixed-case"
        );
    }

    #[test]
    fn test_bearer_token_from_header_invalid() {
        let cases = [
            "",
            "Bearer",
            "Bearer ",
            "Token something",
            "Bearer token extra",
            "bear token",
        ];

        for case in cases {
            assert!(
                bearer_token_from_header(case).is_err(),
                "{case} should fail"
            );
        }
    }

    fn make_dev_user(id: &str, name: &str, email: &str, password: &str, role: Role) -> DevUser {
        let password_hash =
            bcrypt::hash(password, bcrypt::DEFAULT_COST).expect("Failed to hash password");

        DevUser {
            id: id.to_string(),
            name: name.to_string(),
            email: email.to_string(),
            password_hash,
            role,
        }
    }

    #[test]
    fn test_auth_state_dev_mode() {
        let mut config = AuthConfig::default();
        config.dev_mode = true;
        let state = AuthState::new(config);
        assert!(state.is_dev_mode());
    }

    #[test]
    fn test_validate_dev_credentials() {
        let mut config = AuthConfig::default();
        config.dev_mode = true;
        config.dev_users = vec![
            make_dev_user(
                "dev",
                "Developer",
                "dev@localhost",
                "devpassword123",
                Role::Admin,
            ),
            make_dev_user(
                "user",
                "Test User",
                "user@localhost",
                "userpassword123",
                Role::User,
            ),
        ];
        let state = AuthState::new(config);

        // Valid credentials (using the default dev passwords)
        let user = state.validate_dev_credentials("dev", "devpassword123");
        assert!(user.is_some());
        assert_eq!(user.unwrap().role, Role::Admin);

        // Valid email credentials
        let user = state.validate_dev_credentials("user@localhost", "userpassword123");
        assert!(user.is_some());

        // Invalid credentials
        let user = state.validate_dev_credentials("dev", "wrong");
        assert!(user.is_none());
    }

    #[test]
    fn test_generate_and_validate_token() {
        // Create config with a JWT secret for testing
        let mut config = AuthConfig::default();
        config.dev_mode = true;
        config.dev_users = vec![make_dev_user(
            "dev",
            "Developer",
            "dev@localhost",
            "devpassword123",
            Role::Admin,
        )];
        config.jwt_secret = Some("test-secret-for-unit-tests-minimum-32-chars-long".to_string());
        let state = AuthState::new(config);

        let dev_user = &state.dev_users()[0];
        let token = state.generate_dev_token(dev_user).unwrap();

        let claims = state.validate_token(&token).unwrap();
        assert_eq!(claims.sub, dev_user.id);
        assert!(claims.is_admin());
    }

    #[test]
    fn test_dev_token_validation() {
        let mut config = AuthConfig::default();
        config.dev_mode = true;
        config.dev_users = vec![make_dev_user(
            "dev",
            "Developer",
            "dev@localhost",
            "devpassword123",
            Role::Admin,
        )];
        let state = AuthState::new(config);

        // Valid dev token
        let claims = state.validate_token("dev:dev").unwrap();
        assert_eq!(claims.sub, "dev");

        // Invalid dev token (unknown user)
        let result = state.validate_token("dev:unknown");
        assert!(result.is_err());
    }

    #[test]
    fn test_current_user() {
        let claims = Claims {
            sub: "user1".to_string(),
            iss: None,
            aud: None,
            exp: Utc::now().timestamp() + 3600,
            iat: None,
            nbf: None,
            jti: None,
            email: Some("user@example.com".to_string()),
            name: Some("Test User".to_string()),
            preferred_username: None,
            roles: vec!["admin".to_string()],
            role: None,
        };

        let user = CurrentUser { claims };
        assert_eq!(user.id(), "user1");
        assert!(user.is_admin());
        assert_eq!(user.display_name(), "Test User");
    }
}
