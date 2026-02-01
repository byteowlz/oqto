//! API integration tests.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use serde_json::{Value, json};
use tower::ServiceExt;
use urlencoding::encode;
use uuid::Uuid;

mod common;
use common::test_app;

/// Test that health endpoint works without authentication.
#[tokio::test]
async fn test_health_endpoint() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .method(Method::GET)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "ok");
    assert!(json["version"].is_string());
}

/// Test dev login endpoint.
#[tokio::test]
async fn test_dev_login_success() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .method(Method::POST)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "username": "dev",
                        "password": "devpassword123"  // Updated to new password
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(cookie.contains("auth_token="));

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["token"].is_string());
    assert_eq!(json["user"]["id"], "dev");
    assert_eq!(json["user"]["role"], "admin");
}

/// Test dev login with invalid credentials.
#[tokio::test]
async fn test_dev_login_invalid_credentials() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .method(Method::POST)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "username": "dev",
                        "password": "wrong"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Test that protected endpoints require authentication.
#[tokio::test]
async fn test_sessions_requires_auth() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/sessions")
                .method(Method::GET)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Test listing sessions with authentication.
#[tokio::test]
async fn test_list_sessions_with_auth() {
    let (app, token) = common::test_app_with_token().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/sessions")
                .method(Method::GET)
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json.is_array());
}

/// Test listing sessions with dev user header.
#[tokio::test]
async fn test_list_sessions_with_dev_header() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/sessions")
                .method(Method::GET)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

/// Test listing sessions with cookie-based authentication.
#[tokio::test]
async fn test_list_sessions_with_cookie_auth() {
    let app = test_app().await;

    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .method(Method::POST)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "username": "dev",
                        "password": "devpassword123"  // Updated to new password
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(login.status(), StatusCode::OK);

    let set_cookie = login
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    let cookie_pair = set_cookie.split(';').next().unwrap_or_default();
    assert!(cookie_pair.starts_with("auth_token="));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/sessions")
                .method(Method::GET)
                .header(header::COOKIE, cookie_pair)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

/// Test admin endpoints require admin role.
#[tokio::test]
async fn test_admin_sessions_requires_admin() {
    let (app, _token) = common::test_app_with_user_token().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/sessions")
                .method(Method::GET)
                .header("X-Dev-User", "user")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// Test admin endpoints work for admin users.
#[tokio::test]
async fn test_admin_sessions_with_admin() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/sessions")
                .method(Method::GET)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

/// Test getting a non-existent session returns 404.
#[tokio::test]
async fn test_get_nonexistent_session() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/sessions/nonexistent-id")
                .method(Method::GET)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// Test PRD-compatible proxy routes return 404 for unknown sessions.
#[tokio::test]
async fn test_prd_proxy_routes_unknown_session() {
    let app = test_app().await;

    let opencode = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/session/nonexistent/code/session")
                .method(Method::GET)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(opencode.status(), StatusCode::NOT_FOUND);

    let files = app
        .oneshot(
            Request::builder()
                .uri("/session/nonexistent/files/tree")
                .method(Method::GET)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(files.status(), StatusCode::NOT_FOUND);
}

/// Test deleting a non-existent session returns 404.
#[tokio::test]
async fn test_delete_nonexistent_session() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/sessions/nonexistent-id")
                .method(Method::DELETE)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ============================================================================
// User Management Tests
// ============================================================================

/// Test that user list endpoint requires admin role.
#[tokio::test]
async fn test_list_users_requires_admin() {
    let app = test_app().await;

    // Try with regular user
    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/users")
                .method(Method::GET)
                .header("X-Dev-User", "user")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// Test listing users with admin role.
#[tokio::test]
async fn test_list_users_with_admin() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/users")
                .method(Method::GET)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json.is_array());
}

/// Test creating a new user.
#[tokio::test]
async fn test_create_user() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/users")
                .method(Method::POST)
                .header("X-Dev-User", "dev")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "username": "newuser",
                        "email": "newuser@example.com",
                        "password": "password123",
                        "display_name": "New User"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["id"].is_string());
    assert_eq!(json["username"], "newuser");
    assert_eq!(json["email"], "newuser@example.com");
    assert_eq!(json["display_name"], "New User");
    assert_eq!(json["role"], "user");
    assert_eq!(json["is_active"], true);
}

/// Test creating user with duplicate username returns conflict.
#[tokio::test]
async fn test_create_user_duplicate_username() {
    let app = test_app().await;

    // Create first user
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/users")
                .method(Method::POST)
                .header("X-Dev-User", "dev")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "username": "duplicate",
                        "email": "first@example.com",
                        "password": "password123"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(create_response.status(), StatusCode::CREATED);

    // Try to create with same username
    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/users")
                .method(Method::POST)
                .header("X-Dev-User", "dev")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "username": "duplicate",
                        "email": "second@example.com",
                        "password": "password123"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

/// Test creating user with invalid username returns bad request.
#[tokio::test]
async fn test_create_user_invalid_username() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/users")
                .method(Method::POST)
                .header("X-Dev-User", "dev")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "username": "ab",  // too short
                        "email": "user@example.com",
                        "password": "password123"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Test getting user stats.
#[tokio::test]
async fn test_user_stats() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/users/stats")
                .method(Method::GET)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["total"].is_i64());
    assert!(json["admins"].is_i64());
    assert!(json["users"].is_i64());
}

/// Test getting a specific user.
#[tokio::test]
async fn test_get_user() {
    let app = test_app().await;

    // Create a user first
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/users")
                .method(Method::POST)
                .header("X-Dev-User", "dev")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "username": "gettest",
                        "email": "gettest@example.com",
                        "password": "password123"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(create_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let created: Value = serde_json::from_slice(&body).unwrap();
    let user_id = created["id"].as_str().unwrap();

    // Now get the user
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/admin/users/{}", user_id))
                .method(Method::GET)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["id"], user_id);
    assert_eq!(json["username"], "gettest");
}

/// Test getting a non-existent user returns 404.
#[tokio::test]
async fn test_get_nonexistent_user() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/users/nonexistent-id")
                .method(Method::GET)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// Test updating a user.
#[tokio::test]
async fn test_update_user() {
    let app = test_app().await;

    // Create a user first
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/users")
                .method(Method::POST)
                .header("X-Dev-User", "dev")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "username": "updatetest",
                        "email": "updatetest@example.com",
                        "password": "password123"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(create_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let created: Value = serde_json::from_slice(&body).unwrap();
    let user_id = created["id"].as_str().unwrap();

    // Update the user
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/admin/users/{}", user_id))
                .method(Method::PUT)
                .header("X-Dev-User", "dev")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "display_name": "Updated Name",
                        "role": "admin"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["display_name"], "Updated Name");
    assert_eq!(json["role"], "admin");
}

/// Test deleting a user.
#[tokio::test]
async fn test_delete_user() {
    let app = test_app().await;

    // Create a user first
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/users")
                .method(Method::POST)
                .header("X-Dev-User", "dev")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "username": "deletetest",
                        "email": "deletetest@example.com",
                        "password": "password123"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(create_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let created: Value = serde_json::from_slice(&body).unwrap();
    let user_id = created["id"].as_str().unwrap();

    // Delete the user
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/admin/users/{}", user_id))
                .method(Method::DELETE)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify user is gone
    let get_response = app
        .oneshot(
            Request::builder()
                .uri(format!("/admin/users/{}", user_id))
                .method(Method::GET)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
}

/// Test deactivating and activating a user.
#[tokio::test]
async fn test_deactivate_activate_user() {
    let app = test_app().await;

    // Create a user first
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/users")
                .method(Method::POST)
                .header("X-Dev-User", "dev")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "username": "deactivatetest",
                        "email": "deactivatetest@example.com",
                        "password": "password123"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(create_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let created: Value = serde_json::from_slice(&body).unwrap();
    let user_id = created["id"].as_str().unwrap();

    // Deactivate the user
    let deactivate_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/admin/users/{}/deactivate", user_id))
                .method(Method::POST)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(deactivate_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(deactivate_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["is_active"], false);

    // Activate the user
    let activate_response = app
        .oneshot(
            Request::builder()
                .uri(format!("/admin/users/{}/activate", user_id))
                .method(Method::POST)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(activate_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(activate_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["is_active"], true);
}

/// Test getting current user profile.
#[tokio::test]
async fn test_get_me() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/me")
                .method(Method::GET)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["id"], "dev");
}

// ============================================================================
// AgentRPC Tests
// ============================================================================

/// Test that agent health endpoint works when backend is enabled.
#[tokio::test]
async fn test_agent_health_with_backend() {
    let app = common::test_app_with_agent_backend().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/agent/health")
                .method(Method::GET)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["healthy"], true);
    assert_eq!(json["mode"], "mock");
}

/// Test that agent health returns 500 when backend is not enabled.
#[tokio::test]
async fn test_agent_health_without_backend() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/agent/health")
                .method(Method::GET)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

/// Test listing conversations via AgentRPC.
#[tokio::test]
async fn test_agent_list_conversations() {
    let app = common::test_app_with_agent_backend().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/agent/conversations")
                .method(Method::GET)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json.is_array());
    let conversations = json.as_array().unwrap();
    assert_eq!(conversations.len(), 2);
    assert_eq!(conversations[0]["id"], "conv_test1");
    assert_eq!(conversations[1]["id"], "conv_test2");
}

/// Test getting a specific conversation.
#[tokio::test]
async fn test_agent_get_conversation() {
    let app = common::test_app_with_agent_backend().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/agent/conversations/conv_test1")
                .method(Method::GET)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["id"], "conv_test1");
    assert_eq!(json["title"], "Test Conversation 1");
    assert_eq!(json["project_name"], "project1");
}

/// Test getting a non-existent conversation returns 404.
#[tokio::test]
async fn test_agent_get_nonexistent_conversation() {
    let app = common::test_app_with_agent_backend().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/agent/conversations/nonexistent")
                .method(Method::GET)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// Test getting messages for a conversation.
#[tokio::test]
async fn test_agent_get_messages() {
    let app = common::test_app_with_agent_backend().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/agent/conversations/conv_test1/messages")
                .method(Method::GET)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json.is_array());
}

/// Test starting a new agent session.
#[tokio::test]
async fn test_agent_start_session() {
    let app = common::test_app_with_agent_backend().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/agent/sessions")
                .method(Method::POST)
                .header("X-Dev-User", "dev")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "workdir": "/home/test/project"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["session_id"], "ses_mock123");
    assert!(json["opencode_port"].is_number());
    assert_eq!(json["is_new"], true);
}

/// Test sending a message to a session.
#[tokio::test]
async fn test_agent_send_message() {
    let app = common::test_app_with_agent_backend().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/agent/sessions/ses_mock123/messages")
                .method(Method::POST)
                .header("X-Dev-User", "dev")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "text": "Hello, agent!"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

/// Test stopping a session.
#[tokio::test]
async fn test_agent_stop_session() {
    let app = common::test_app_with_agent_backend().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/agent/sessions/ses_mock123")
                .method(Method::DELETE)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

/// Test getting session URL.
#[tokio::test]
async fn test_agent_get_session_url() {
    let app = common::test_app_with_agent_backend().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/agent/sessions/ses_mock123/url")
                .method(Method::GET)
                .header("X-Dev-User", "dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["session_id"], "ses_mock123");
    assert!(json["url"].is_string());
}

/// Test that agent endpoints require authentication.
#[tokio::test]
async fn test_agent_endpoints_require_auth() {
    let app = common::test_app_with_agent_backend().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/agent/conversations")
                .method(Method::GET)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_pi_settings_workspace_scope_does_not_mutate_global() {
    let (app, token) = common::test_app_with_token().await;

    let workspace_root = std::env::temp_dir()
        .join("octo-tests-workspaces")
        .join("dev");
    let workspace_dir = workspace_root.join(format!("pi-settings-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&workspace_dir).unwrap();

    let global_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/settings?app=pi-agent")
                .method(Method::GET)
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(global_response.status(), StatusCode::OK);

    let global_body = axum::body::to_bytes(global_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let global_values: Value = serde_json::from_slice(&global_body).unwrap();
    assert_eq!(global_values["defaultProvider"]["is_configured"], false);
    assert!(global_values["defaultProvider"]["value"].is_null());

    let update_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/settings?app=pi-agent&workspace_path={}",
                    encode(workspace_dir.to_string_lossy().as_ref())
                ))
                .method(Method::PATCH)
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "values": { "defaultProvider": "anthropic" }
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(update_response.status(), StatusCode::OK);

    let update_body = axum::body::to_bytes(update_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let update_values: Value = serde_json::from_slice(&update_body).unwrap();
    assert_eq!(update_values["defaultProvider"]["value"], "anthropic");
    assert_eq!(update_values["defaultProvider"]["is_configured"], true);

    let workspace_settings = workspace_dir.join(".pi").join("settings.json");
    assert!(workspace_settings.exists());

    let settings_content = std::fs::read_to_string(&workspace_settings).unwrap();
    let settings_json: Value = serde_json::from_str(&settings_content).unwrap();
    assert_eq!(settings_json["defaultProvider"], "anthropic");

    let global_after_response = app
        .oneshot(
            Request::builder()
                .uri("/settings?app=pi-agent")
                .method(Method::GET)
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(global_after_response.status(), StatusCode::OK);

    let global_after_body = axum::body::to_bytes(global_after_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let global_after_values: Value = serde_json::from_slice(&global_after_body).unwrap();
    assert_eq!(
        global_after_values["defaultProvider"]["is_configured"],
        false
    );
    assert!(global_after_values["defaultProvider"]["value"].is_null());
}
