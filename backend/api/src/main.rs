mod config;
mod error;
mod middleware;
mod routes;
mod state;
mod sync_impl;

use axum::{
    http::{header, HeaderValue, StatusCode},
    middleware as axum_middleware,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Extension, Router,
};
use mailquill_core::{blob::create_blob_store, crypto::CredentialKey, jwt::JwtKey, pii::init_pii_mode};
use db::pool::UserDbPool;
use imap_sync::manager::SyncManager;
use rust_embed::RustEmbed;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::sync::Arc;
use tower_http::{set_header::SetResponseHeaderLayer, trace::TraceLayer};
use web_push::IsahcWebPushClient;

use state::{AppState, VapidConfig};

#[derive(RustEmbed)]
#[folder = "../../frontend/dist/"]
#[allow(dead_code)]
struct FrontendAssets;

const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self' data: blob:; connect-src 'self'; frame-src 'none'; object-src 'none'";

#[tokio::main]
async fn main() {
    let settings = config::Settings::load();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    init_pii_mode();

    let credential_key = Arc::new(CredentialKey::from_hex(&settings.credential_encryption_key));
    let jwt_key = Arc::new(JwtKey::from_secret(settings.jwt_secret.as_bytes()));

    let data_dir = settings.data_dir.clone();
    let app_db = open_app_db(&data_dir).await;
    let blob_store = create_blob_store();
    let user_db_pool = UserDbPool::new(&data_dir);

    let sync_manager = Arc::new(SyncManager::new());
    let vapid = load_vapid_config();
    let web_push_client = vapid
        .as_ref()
        .and_then(|_| match IsahcWebPushClient::new() {
            Ok(client) => Some(Arc::new(client)),
            Err(e) => {
                tracing::warn!("web push client disabled: {e}");
                None
            }
        });

    // Re-start sync tasks for existing accounts
    restart_existing_accounts(&sync_manager, &user_db_pool, &data_dir).await;

    let pkce_store = Arc::new(routes::oauth::new_pkce_store());

    let state = AppState {
        app_db,
        user_db_pool,
        blob_store,
        sync_manager,
        credential_key,
        jwt_key,
        vapid,
        web_push_client,
    };

    let auth_routes = Router::new()
        .route("/auth/register", post(routes::auth::register))
        .route("/auth/login", post(routes::auth::login))
        .route("/auth/refresh", post(routes::auth::refresh))
        .route("/auth/logout", post(routes::auth::logout))
        .route("/auth/oauth/{provider}/start", get(routes::oauth::oauth_start).layer(axum_middleware::from_fn_with_state(state.clone(), middleware::require_auth)))
        .route("/auth/oauth/{provider}/callback", get(routes::oauth::oauth_callback))
        .route("/auth/me", get(routes::auth::me).layer(axum_middleware::from_fn_with_state(state.clone(), middleware::require_auth)));

    let protected = Router::new()
        // Accounts
        .route("/accounts", post(routes::accounts::add_account).get(routes::accounts::list_accounts))
        .route("/accounts/{id}", get(routes::accounts::get_account).put(routes::accounts::update_account).delete(routes::accounts::delete_account))
        .route("/accounts/{id}/sync", post(routes::accounts::trigger_sync))
        .route("/accounts/{id}/sync-status", get(routes::accounts::sync_status))
        .route("/accounts/{id}/aliases", post(routes::accounts::add_alias).get(routes::accounts::list_aliases))
        .route("/accounts/{id}/aliases/{alias_id}", patch(routes::accounts::update_alias).delete(routes::accounts::delete_alias))
        .route("/accounts/{id}/folders", get(routes::mailbox::list_folders))
        .route("/accounts/{id}/folders/{folder}/messages", get(routes::mailbox::list_folder_messages))
        // Mailbox
        .route("/mailbox/unified", get(routes::mailbox::unified_inbox))
        // Messages
        .route("/messages/{id}", get(routes::messages::get_message))
        .route("/messages/{id}/read", patch(routes::messages::mark_read))
        .route("/messages/{id}/flag", patch(routes::messages::toggle_flag))
        .route("/messages/{id}/archive", post(routes::messages::archive_message))
        .route("/messages/{id}", delete(routes::messages::delete_message))
        .route("/messages/{id}/move", post(routes::messages::move_message))
        // Threads
        .route("/threads/{thread_id}", get(routes::threads::get_thread))
        .route("/threads/{thread_id}/archive", post(routes::threads::archive_thread))
        .route("/threads/{thread_id}/delete", post(routes::threads::delete_thread))
        .route("/threads/{thread_id}/read", patch(routes::threads::mark_thread_read))
        // Send
        .route("/send", post(routes::send::send_email))
        // Search
        .route("/search", get(routes::search::search))
        // Settings
        .route("/settings", get(routes::settings::get_settings).patch(routes::settings::patch_settings))
        // Push subscriptions
        .route("/push-subscriptions/vapid-public-key", get(routes::push_subscriptions::vapid_public_key))
        .route("/push-subscriptions", post(routes::push_subscriptions::create_subscription))
        .route("/push-subscriptions/{id}", delete(routes::push_subscriptions::delete_subscription))
        .layer(axum_middleware::from_fn_with_state(state.clone(), middleware::require_auth));

    let app = Router::new()
        .route("/api/health", get(health))
        .nest("/api", auth_routes)
        .nest("/api", protected)
        .fallback(serve_frontend)
        .layer(Extension(pkce_store))
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CSP),
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: std::net::SocketAddr = format!("{}:{}", settings.server_host, settings.server_port)
        .parse()
        .expect("invalid server_host/server_port");
    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn load_vapid_config() -> Option<Arc<VapidConfig>> {
    let public_key = std::env::var("VAPID_PUBLIC_KEY").ok().filter(|v| !v.is_empty());
    let private_key = std::env::var("VAPID_PRIVATE_KEY").ok().filter(|v| !v.is_empty());
    let subject = std::env::var("VAPID_SUBJECT")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "mailto:admin@example.com".to_owned());

    match (public_key, private_key) {
        (Some(public_key), Some(private_key)) => Some(Arc::new(VapidConfig {
            public_key,
            private_key,
            subject,
        })),
        _ => {
            tracing::warn!("web push disabled: VAPID_PUBLIC_KEY or VAPID_PRIVATE_KEY is missing");
            None
        }
    }
}

async fn open_app_db(data_dir: &str) -> SqlitePool {
    let db_path = format!("{data_dir}/app.db");
    let opts = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(opts).await.expect("open app.db");
    sqlx::query(db::migrations::WAL_PRAGMAS)
        .execute(&pool)
        .await
        .expect("apply WAL pragmas");
    db::migrations::run_app_migrations(&pool)
        .await
        .expect("run app migrations");
    pool
}

/// Re-start sync tasks for all existing user accounts on server restart.
async fn restart_existing_accounts(
    sync_manager: &Arc<SyncManager>,
    user_db_pool: &Arc<UserDbPool>,
    data_dir: &str,
) {
    use std::path::Path;
    let users_dir = Path::new(data_dir).join("users");
    if !users_dir.exists() {
        return;
    }
    let mut dir = match tokio::fs::read_dir(&users_dir).await {
        Ok(d) => d,
        Err(_) => return,
    };
    while let Ok(Some(entry)) = dir.next_entry().await {
        let user_id = entry.file_name().to_string_lossy().into_owned();
        let db = match user_db_pool.get(&user_id).await {
            Ok(db) => db,
            Err(_) => continue,
        };
        let account_ids: Vec<String> =
            sqlx::query_scalar("SELECT id FROM email_accounts")
                .fetch_all(&db)
                .await
                .unwrap_or_default();
        for account_id in account_ids {
            sync_manager.start_account_minimal(account_id, user_id.clone()).await;
        }
    }
}

async fn health() -> impl IntoResponse {
    axum::Json(serde_json::json!({ "status": "ok" }))
}

async fn serve_frontend(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match FrontendAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let cache_control = if path == "index.html" {
                "no-cache"
            } else {
                "public, max-age=31536000, immutable"
            };
            axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("content-type", mime.as_ref())
                .header("cache-control", cache_control)
                .header("content-security-policy", CSP)
                .body(axum::body::Body::from(content.data))
                .unwrap()
        }
        None => match FrontendAssets::get("index.html") {
            Some(content) => axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/html")
                .header("cache-control", "no-cache")
                .header("content-security-policy", CSP)
                .body(axum::body::Body::from(content.data))
                .unwrap(),
            None => axum::response::Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(axum::body::Body::from("Not Found"))
                .unwrap(),
        },
    }
}
