//! Synthetic loopback proof of control/inference routing and credential separation.
use axum::{
    Json, Router,
    body::Body,
    http::{Request, StatusCode, header},
    middleware::{self, Next},
    response::IntoResponse,
    routing::{get, post},
};
use oqto_eavs::{EavsClient, generate_pi_models_json};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const OPERATOR: &str = "synthetic-operator-secret";
const LISTENER: &str = "synthetic-listener-capability";
const SCOPED: &str = "eavs-synthetic-scoped";

struct TestServer {
    url: String,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
}
impl TestServer {
    async fn start(router: Router) -> anyhow::Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let task = tokio::spawn(async move { axum::serve(listener, router).await });
        Ok(Self { url, task })
    }
}
impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn control_router(calls: Arc<AtomicUsize>) -> Router {
    Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/admin/keys", get(|| async { Json(json!([])) }))
        .route("/auth/status/synthetic", get(|| async { Json(json!([])) }))
        .route(
            "/catalog/lookup",
            get(
                |axum::extract::Query(query): axum::extract::Query<
                    std::collections::HashMap<String, String>,
                >| async move {
                    assert_eq!(
                        query.get("model_id").map(String::as_str),
                        Some("fixture/model")
                    );
                    assert_eq!(query.get("provider").map(String::as_str), Some("fixture"));
                    Json(json!({"synthetic":"catalog"}))
                },
            ),
        )
        .route(
            "/providers/detail",
            get(|| async {
                Json(json!([{
                    "name":"fixture", "type":"openai-compatible", "pi_api":"openai-completions",
                    "oauth":false, "has_api_key":true, "models":[{"id":"fixture-model"}]
                }]))
            }),
        )
        .layer(middleware::from_fn(
            move |request: Request<Body>, next: Next| {
                let calls = calls.clone();
                async move {
                    let bearer = format!("Bearer {OPERATOR}");
                    if request
                        .headers()
                        .get(header::AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        != Some(bearer.as_str())
                        || request
                            .headers()
                            .get("x-eavs-token")
                            .and_then(|value| value.to_str().ok())
                            != Some(LISTENER)
                    {
                        return StatusCode::UNAUTHORIZED.into_response();
                    }
                    calls.fetch_add(1, Ordering::SeqCst);
                    next.run(request).await
                }
            },
        ))
}

fn inference_router(calls: Arc<AtomicUsize>) -> Router {
    Router::new().route(
        "/fixture/v1/chat/completions",
        post(move |request: Request<Body>| {
            let calls = calls.clone();
            async move {
                let bearer = format!("Bearer {SCOPED}");
                if request
                    .headers()
                    .get(header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    != Some(bearer.as_str())
                    || request.headers().contains_key("x-eavs-token")
                {
                    return (StatusCode::UNAUTHORIZED, Json(json!({})));
                }
                calls.fetch_add(1, Ordering::SeqCst);
                (StatusCode::OK, Json(json!({"synthetic":true})))
            }
        }),
    )
}

#[tokio::test]
async fn operator_redirects_are_not_followed() -> anyhow::Result<()> {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let target = TestServer::start(Router::new().fallback(move || {
        count.fetch_add(1, Ordering::SeqCst);
        async { StatusCode::OK }
    }))
    .await?;
    let location = format!("{}/operator-tripwire", target.url);
    let control = TestServer::start(Router::new().route(
        "/health",
        get(move || {
            let location = location.clone();
            async move {
                (
                    StatusCode::TEMPORARY_REDIRECT,
                    [(header::LOCATION, location)],
                )
            }
        }),
    ))
    .await?;
    let client = EavsClient::with_endpoints(&control.url, &target.url, OPERATOR)?;
    assert!(!client.health_check().await?);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn control_calls_authenticate_but_generated_models_use_scoped_inference() -> anyhow::Result<()>
{
    let operator_calls = Arc::new(AtomicUsize::new(0));
    let inference_calls = Arc::new(AtomicUsize::new(0));
    let control = TestServer::start(control_router(operator_calls.clone())).await?;
    let inference = TestServer::start(inference_router(inference_calls.clone())).await?;
    let client = EavsClient::with_endpoints_and_listener_token(
        &control.url,
        &inference.url,
        OPERATOR,
        Some(LISTENER),
    )?;
    assert!(client.health_check().await?);
    assert!(client.list_keys().await?.is_empty());
    assert!(client.oauth_status("synthetic").await?.is_empty());
    assert_eq!(
        client
            .catalog_lookup("fixture/model", Some("fixture"))
            .await?,
        json!({"synthetic":"catalog"})
    );
    let providers = client.providers_detail().await?;
    let models = generate_pi_models_json(&providers, client.inference_url(), Some(SCOPED));
    let model_base = models["providers"]["eavs-fixture"]["baseUrl"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing generated inference URL"))?;
    assert_eq!(model_base, format!("{}/fixture/v1", inference.url));
    for private in [OPERATOR, LISTENER, control.url.as_str()] {
        assert!(!models.to_string().contains(private));
    }
    let result = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?
        .post(format!("{model_base}/chat/completions"))
        .bearer_auth(SCOPED)
        .json(&json!({"model":"fixture-model"}))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    assert_eq!(result, json!({"synthetic":true}));
    assert_eq!(operator_calls.load(Ordering::SeqCst), 5);
    assert_eq!(inference_calls.load(Ordering::SeqCst), 1);
    Ok(())
}
