use rszero::prelude::*;
use serde::{Deserialize, Serialize};
use axum::routing::get;
use axum::Router;
use axum::Json;
use std::sync::Arc;

// ─── Request/Response Types ───────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
struct User {
    id: i64,
    name: String,
    age: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateUserReq {
    name: String,
    age: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateUserReq {
    name: Option<String>,
    age: Option<i32>,
}

// ─── In-Memory Store (production would use rszero::store::Store) ──────────

use std::sync::Mutex;

struct AppState {
    users: Mutex<Vec<User>>,
    next_id: Mutex<i64>,
    metrics: Arc<Metrics>,
}

// ─── Handlers ─────────────────────────────────────────────────────────────

async fn list_users(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> impl axum::response::IntoResponse {
    let _guard = state.metrics.start_request("GET", "/users");
    let users = state.users.lock().unwrap().clone();
    JsonResponse::ok(users)
}

async fn get_user(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> impl axum::response::IntoResponse {
    let _guard = state.metrics.start_request("GET", "/users/:id");
    let users = state.users.lock().unwrap();
    match users.iter().find(|u| u.id == id) {
        Some(user) => JsonResponse::ok(user.clone()),
        None => JsonResponse::error(404, "user not found"),
    }
}

async fn create_user(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<CreateUserReq>,
) -> impl axum::response::IntoResponse {
    let _guard = state.metrics.start_request("POST", "/users");
    let id = {
        let mut next = state.next_id.lock().unwrap();
        let id = *next;
        *next += 1;
        id
    };
    let user = User {
        id,
        name: req.name,
        age: req.age,
    };
    state.users.lock().unwrap().push(user.clone());
    JsonResponse::ok(user)
}

async fn update_user(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    Json(req): Json<UpdateUserReq>,
) -> impl axum::response::IntoResponse {
    let _guard = state.metrics.start_request("PUT", "/users/:id");
    let mut users = state.users.lock().unwrap();
    match users.iter_mut().find(|u| u.id == id) {
        Some(user) => {
            if let Some(name) = req.name { user.name = name; }
            if let Some(age) = req.age { user.age = age; }
            JsonResponse::ok(user.clone())
        }
        None => JsonResponse::error(404, "user not found"),
    }
}

async fn delete_user(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> impl axum::response::IntoResponse {
    let _guard = state.metrics.start_request("DELETE", "/users/:id");
    let mut users = state.users.lock().unwrap();
    let before = users.len();
    users.retain(|u| u.id != id);
    if users.len() < before {
        JsonResponse::ok_empty()
    } else {
        JsonResponse::error(404, "user not found")
    }
}

async fn health_check() -> impl axum::response::IntoResponse {
    JsonResponse::ok(serde_json::json!({"status": "healthy"}))
}

// ─── Main ─────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = load_config("etc/api.yaml")?;
    log::init(&config.log);

    let metrics = Arc::new(Metrics::new("user-service"));

    let state = Arc::new(AppState {
        users: Mutex::new(vec![
            User { id: 1, name: "Alice".into(), age: 30 },
            User { id: 2, name: "Bob".into(), age: 25 },
        ]),
        next_id: Mutex::new(3),
        metrics: metrics.clone(),
    });

    let api_routes = Router::new()
        .route("/health", get(health_check))
        .route("/users", get(list_users).post(create_user))
        .route("/users/:id", get(get_user).put(update_user).delete(delete_user))
        .with_state(state.clone());

    let server = RszeroServer::new(&config.host, config.port)
        .with_metrics(metrics.clone())
        .merge_router(api_routes);

    tracing::info!("user-service starting on {}:{}", config.host, config.port);
    server.start().await?;
    Ok(())
}
