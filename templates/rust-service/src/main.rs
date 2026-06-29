use {{CRATE}}::db;
use {{CRATE}}::http::Http;
use {{CRATE}}::routes::{self, AppState};
use {{CRATE}}::tasks::Tasks;

#[tokio::main]
async fn main() {
    // Logs to stderr; level from RUST_LOG (defaults to info).
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let http = Http::new();
    let tasks = Tasks::new();

    // The server starts even without a database, so liveness passes while the pool warms up.
    let pool = match std::env::var("DATABASE_URL") {
        Ok(url) => match db::connect(&url).await {
            Ok(p) => {
                tracing::info!("connected to database");
                Some(p)
            }
            Err(e) => {
                tracing::warn!("database not ready, starting anyway: {e}");
                None
            }
        },
        Err(_) => {
            tracing::info!("DATABASE_URL unset; starting without a database");
            None
        }
    };

    let app = routes::build(AppState { pool, http, tasks: tasks.clone() });

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    tracing::info!("listening on http://{addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");

    // HTTP has drained; finish tracked background work (bounded), so nothing is dropped on deploy.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(10), tasks.shutdown()).await;
}

/// Graceful shutdown on Ctrl-C / SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("install ctrl_c handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received, draining");
}
