use std::sync::Arc;

use axum::{http::HeaderName, middleware, routing::get, Router};
use axum::http::Method;
use tokio::{net::TcpListener, task::JoinHandle};
use tower_http::cors::{Any, CorsLayer};

use crate::{auth, config::Config, db, handlers::health};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: db::DbPool,
}

pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
        ])
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .expose_headers([HeaderName::from_static("content-type")]);

    Router::new()
        .route("/health", get(health::health))
        .route("/protected", get(|| async { "protected ok" }))
        .route("/exercises", get(crate::handlers::pull::pull_exercises))
        .route("/ingredients", get(crate::handlers::pull::pull_ingredients).post(crate::handlers::push::push_ingredient))
        .route("/fx-rates", get(crate::handlers::pull::pull_fx_rates))
        .route("/workouts", axum::routing::post(crate::handlers::push::push_workouts))
        .route("/templates", axum::routing::post(crate::handlers::push::push_templates))
        .route("/notes", axum::routing::post(crate::handlers::push::push_notes))
        .route("/tasks", axum::routing::post(crate::handlers::push::push_tasks))
        .route("/goals", axum::routing::post(crate::handlers::push::push_goals))
        .route("/meals", axum::routing::post(crate::handlers::push::push_meals))
        .route("/transactions", axum::routing::post(crate::handlers::push::push_transactions))
        .route("/budget-accounts", axum::routing::post(crate::handlers::push::push_budget_accounts))
        .route("/receipt-pictures", axum::routing::post(crate::handlers::receipts::push_receipt_pictures))
        .route("/backup", axum::routing::post(crate::handlers::backup::post_backup))
        .route("/backup/latest", get(crate::handlers::backup::get_backup))
        .route("/media/:id", get(crate::handlers::media::get_media))
        .route("/exercises/:id", get(crate::handlers::media::get_media))
        .layer(middleware::from_fn_with_state(state.clone(), auth::bearer_auth))
        .layer(cors)
        .with_state(state)
}

pub async fn start_server(config: Config) -> anyhow::Result<(u16, JoinHandle<()>)> {
    let db = db::init_db(&config.db_path)?;
    let listener = TcpListener::bind(&config.bind_addr).await?;
    let port = listener.local_addr()?.port();
    let state = AppState {
        config: Arc::new(config),
        db,
    };
    let router = create_router(state);
    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    Ok((port, handle))
}

#[cfg(test)]
pub async fn start_test_server() -> (u16, JoinHandle<()>) {
    start_test_server_with_db(db::init_memory().unwrap()).await
}

#[cfg(test)]
pub async fn start_test_server_with_db(db: db::DbPool) -> (u16, JoinHandle<()>) {
    let config = Config::default();
    let listener = TcpListener::bind(&config.bind_addr).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let state = AppState {
        config: Arc::new(config),
        db,
    };
    let router = create_router(state);
    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (port, handle)
}
