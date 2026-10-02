//! FigureSearch web server.
//!
//! - `GET /api/check` — store reachability (cached for a few minutes)
//! - `GET /api/search?id=…&term=…&stores=a,b` — server-sent events: one `store`
//!   event per store as it finishes, then `done` (or `error`)
//! - everything else — the built frontend (`DIST_DIR`, default `dist`)

use axum::extract::Query;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use search_service::SearchHooks;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore, mpsc};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::Stream;
use tower_http::compression::CompressionLayer;
use tower_http::services::{ServeDir, ServeFile};

/// Searches running at once; each one already fans out to several stores.
const MAX_SEARCHES: usize = 4;
const MAX_TERM_CHARS: usize = 100;
const CHECK_TTL: Duration = Duration::from_secs(10 * 60);

fn search_slots() -> &'static Semaphore {
    static SLOTS: OnceLock<Semaphore> = OnceLock::new();
    SLOTS.get_or_init(|| Semaphore::new(MAX_SEARCHES))
}

type CheckCache = Mutex<Option<(Instant, serde_json::Value)>>;

fn check_cache() -> &'static CheckCache {
    static CACHE: OnceLock<CheckCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

async fn check() -> impl IntoResponse {
    // Holding the lock while checking makes concurrent visitors share one run.
    let mut cache = check_cache().lock().await;
    let fresh = cache.as_ref().filter(|(at, _)| at.elapsed() < CHECK_TTL).map(|(_, checks)| checks.clone());
    let checks = match fresh {
        Some(checks) => checks,
        None => {
            let checks = serde_json::to_value(search_service::check_sites().await).unwrap_or_default();
            *cache = Some((Instant::now(), checks.clone()));
            checks
        }
    };
    Json(checks)
}

#[derive(Deserialize)]
struct SearchParams {
    id: String,
    term: String,
    /// Comma-separated store names.
    stores: String,
}

fn event(name: &str, data: &impl serde::Serialize) -> Result<Event, Infallible> {
    Ok(Event::default().event(name).data(serde_json::to_string(data).unwrap_or_default()))
}

async fn search(Query(params): Query<SearchParams>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::unbounded_channel::<Result<Event, Infallible>>();
    tokio::spawn(async move {
        let Ok(_slot) = search_slots().try_acquire() else {
            let _ = tx.send(event("error", &"지금 검색이 많아요. 잠시 후 다시 시도해 주세요."));
            return;
        };
        if params.term.chars().count() > MAX_TERM_CHARS {
            let _ = tx.send(event("error", &"검색어가 너무 길어요."));
            return;
        }
        let stores = params.stores.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect();
        let (current, sender) = (tx.clone(), tx.clone());
        let hooks = SearchHooks {
            capture_dir: None,
            // The browser closing the connection drops the receiver and stops the search.
            is_current: Arc::new(move || !current.is_closed()),
            on_result: Arc::new(move |result| {
                let _ = sender.send(event("store", &result));
            }),
        };
        let _ = match search_service::search_stores(params.id, params.term, stores, hooks).await {
            Ok(summary) => tx.send(event("done", &summary)),
            Err(message) => tx.send(event("error", &message)),
        };
    });
    Sse::new(UnboundedReceiverStream::new(rx)).keep_alive(KeepAlive::default())
}

async fn health() -> &'static str {
    "ok"
}

fn app(dist: &str) -> Router {
    let index = format!("{dist}/index.html");
    Router::new()
        .route("/api/check", get(check))
        .route("/api/search", get(search))
        .route("/healthz", get(health))
        .fallback_service(ServeDir::new(dist).fallback(ServeFile::new(index)))
        .layer(CompressionLayer::new())
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let port = std::env::var("PORT").ok().and_then(|p| p.parse::<u16>().ok()).unwrap_or(8080);
    let dist = std::env::var("DIST_DIR").unwrap_or_else(|_| "dist".into());
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.expect("failed to bind port");
    log::info!("listening on :{port}, serving {dist}");
    axum::serve(listener, app(&dist))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("server error");
}
