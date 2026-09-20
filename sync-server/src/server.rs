use std::{net::SocketAddr, sync::Arc};

use axum::{extract::ConnectInfo, http::HeaderName, middleware, routing::get, Router};
use axum::http::Method;
use tokio::{net::TcpListener, task::JoinHandle};
use tower_http::cors::{Any, CorsLayer};

use crate::{auth, config::Config, db, handlers::health, logs};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: db::DbPool,
    pub logs: logs::LogBuffer,
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
        .route("/ingredients/lookup", get(crate::handlers::lookup::lookup_ingredient))
        .route("/ingredients/import-from-barcode", axum::routing::post(crate::handlers::lookup::import_from_barcode))
        .route("/fx-rates", get(crate::handlers::pull::pull_fx_rates))
        .route("/workouts", get(crate::handlers::read::pull_workouts).post(crate::handlers::push::push_workouts))
        .route("/templates", axum::routing::post(crate::handlers::push::push_templates))
        .route("/notes", axum::routing::post(crate::handlers::push::push_notes))
        .route("/tasks", axum::routing::post(crate::handlers::push::push_tasks))
        .route("/goals", axum::routing::post(crate::handlers::push::push_goals))
        .route("/meals", get(crate::handlers::read::pull_meals).post(crate::handlers::push::push_meals))
        .route("/body-metrics", get(crate::handlers::read::pull_body_metrics))
        .route("/transactions", axum::routing::post(crate::handlers::push::push_transactions))
        .route("/budget-accounts", axum::routing::post(crate::handlers::push::push_budget_accounts))
        .route("/receipt-pictures", axum::routing::post(crate::handlers::receipts::push_receipt_pictures))
        .route("/backup", axum::routing::post(crate::handlers::backup::post_backup))
        .route("/backup/latest", get(crate::handlers::backup::get_backup))
        .route("/media/:id", get(crate::handlers::media::get_media))
        .route("/exercises/:id", get(crate::handlers::media::get_media))
        .layer(middleware::from_fn_with_state(state.clone(), log_requests))
        .layer(middleware::from_fn_with_state(state.clone(), auth::bearer_auth))
        .layer(cors)
        .with_state(state)
}

async fn log_requests(
    axum::extract::State(state): axum::extract::State<AppState>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> axum::response::Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let peer = connect_info
        .map(|ci| ci.0.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let is_local = peer == "127.0.0.1" || peer == "::1";
    let peer_label = if is_local {
        format!("{} (local)", peer)
    } else {
        peer.clone()
    };
    let auth_present = req.headers().contains_key(axum::http::header::AUTHORIZATION);
    let auth_hint = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| {
            if s.len() > 12 {
                format!("{}...", &s[..12])
            } else {
                "present".to_string()
            }
        })
        .unwrap_or_else(|| "none".to_string());
    let start = std::time::Instant::now();
    let res = next.run(req).await;
    let elapsed = start.elapsed().as_millis();
    let status = res.status().as_u16();
    let line = format!(
        "{} {} from {} {}ms auth={} -> {} [{}]",
        method,
        path,
        peer_label,
        elapsed,
        auth_hint,
        if auth_present { "present" } else { "none" },
        status
    );
    tracing::info!("{}", line);
    logs::push_log(&state.logs, line);
    res
}

pub async fn start_server(config: Config) -> anyhow::Result<(u16, JoinHandle<()>)> {
    let _ = tracing_subscriber::fmt::try_init();
    let db = db::init_db(&config.db_path)?;
    let listener = TcpListener::bind(&config.bind_addr).await?;
    let port = listener.local_addr()?.port();
    let state = AppState {
        config: Arc::new(config),
        db,
        logs: logs::new_buffer(200),
    };
    let router = create_router(state);
    let handle = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
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
        logs: logs::new_buffer(200),
    };
    let router = create_router(state);
    let handle = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    (port, handle)
}
