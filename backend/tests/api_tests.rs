//! API integration tests.

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

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
    
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
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
                        "password": "dev"
                    })).unwrap()
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
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
                    })).unwrap()
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
    
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
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
