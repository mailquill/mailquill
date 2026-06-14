use api::{config, middleware, routes};

use axum::{
    http::{header, HeaderValue, StatusCode},
    middleware as axum_middleware,
    response::IntoResponse,
    routing::{delete, get, patch, post, put},
    Extension, Router,
};
use mailquill_core::{blob::create_blob_store, crypto::CredentialKey, jwt::JwtKey, pii::init_pii_mode};
use db::pool::UserDbPool;
use mail_sync::manager::SyncManager;
use rust_embed::RustEmbed;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::sync::Arc;
use tower_http::{set_header::SetResponseHeaderLayer, trace::TraceLayer};
use web_push::IsahcWebPushClient;

use api::state::{AppState, VapidConfig};

#[derive(RustEmbed)]
#[folder = "../../frontend/dist/"]
#[allow(dead_code)]
struct FrontendAssets;

const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self' data: blob:; connect-src 'self'; frame-src 'none'; object-src 'none'";

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mailquill", about = "Mailquill — self-hosted web mail server")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the API + web server (default).
    Serve,
    /// Generate a Web Push (VAPID) key pair and print it as env lines.
    VapidKeys,
    /// Generate CREDENTIAL_ENCRYPTION_KEY and JWT_SECRET as env lines.
    Secrets,
}

#[tokio::main]
async fn main() {
    match Cli::parse().command {
        Some(Command::VapidKeys) => {
            api::vapid_keys::print_generated();
            return;
        }
        Some(Command::Secrets) => {
            api::secrets::print_generated();
            return;
        }
        Some(Command::Serve) | None => {}
    }

    let settings = config::Settings::load();

    // Sensible default when RUST_LOG is unset: our crates at debug, noisy
    // dependency wire logs (IMAP/TLS/HTTP) capped at warn. RUST_LOG overrides.
    let log_filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new(
            "info,api=debug,mail_sync=debug,smtp=debug,async_imap=warn,imap_proto=warn,\
             async_native_tls=warn,rustls=warn,hyper=warn,hyper_util=warn,h2=warn,sqlx=warn,\
             tower_http=info,mio=warn,want=warn",
        )
    });
    tracing_subscriber::fmt().with_env_filter(log_filter).init();

    init_pii_mode();

    let credential_key = Arc::new(CredentialKey::from_hex(&settings.credential_encryption_key));
    let jwt_key = Arc::new(JwtKey::from_secret(settings.jwt_secret.as_bytes()));

    let data_dir = settings.data_dir.clone();
    phishing::init(
        &data_dir,
        settings.openphish_enabled.then(|| settings.openphish_feed_url.clone()),
    )
    .await;
    let app_db = open_app_db(&data_dir).await;
    let blob_store = create_blob_store();
    let user_db_pool = UserDbPool::new(&data_dir);

    let sync_manager = Arc::new(SyncManager::new());
    let vapid = load_vapid_config(&settings);
    let web_push_client = vapid
        .as_ref()
        .and_then(|_| match IsahcWebPushClient::new() {
            Ok(client) => Some(Arc::new(client)),
            Err(e) => {
                tracing::warn!("web push client disabled: {e}");
                None
            }
        });

    let pkce_store = Arc::new(routes::oauth::new_pkce_store());
    let (events, _) = tokio::sync::broadcast::channel(256);

    let state = AppState {
        app_db,
        user_db_pool,
        blob_store,
        sync_manager,
        credential_key,
        jwt_key,
        vapid,
        web_push_client,
        events,
    };

    // Re-start sync tasks for existing accounts (needs the assembled AppState as
    // the SyncAppState the sync tasks run against).
    restart_existing_accounts(&state, &data_dir).await;

    let auth_routes = Router::new()
        .route("/auth/register", post(routes::auth::register))
        .route("/auth/login", post(routes::auth::login))
        .route("/auth/refresh", post(routes::auth::refresh))
        .route("/auth/logout", post(routes::auth::logout))
        // `oauth_start` is a top-level browser redirect, so it cannot carry an
        // Authorization header — it authenticates from a short-lived `token`
        // query parameter validated inside the handler instead.
        .route("/auth/oauth/{provider}/start", get(routes::oauth::oauth_start))
        .route("/auth/oauth/{provider}/callback", get(routes::oauth::oauth_callback))
        // SSE: EventSource can't set an Authorization header, so this validates
        // a `?token=` query parameter itself (like oauth_start above).
        .route("/events", get(routes::events::events_stream))
        .route("/auth/me", get(routes::auth::me).layer(axum_middleware::from_fn_with_state(state.clone(), middleware::require_auth)));

    let protected = Router::new()
        // Accounts
        .route("/accounts", post(routes::accounts::add_account).get(routes::accounts::list_accounts))
        .route("/accounts/{id}", get(routes::accounts::get_account).put(routes::accounts::update_account).delete(routes::accounts::delete_account))
        .route("/accounts/{id}/sync", post(routes::accounts::trigger_sync))
        .route("/accounts/{id}/sync-status", get(routes::accounts::sync_status))
        .route("/accounts/{id}/aliases", post(routes::accounts::add_alias).get(routes::accounts::list_aliases))
        .route("/accounts/{id}/aliases/{alias_id}", patch(routes::accounts::update_alias).delete(routes::accounts::delete_alias))
        .route("/accounts/{id}/sync-dav", post(routes::dav::sync_dav))
        .route("/accounts/{id}/folders", get(routes::mailbox::list_folders))
        .route("/accounts/{id}/folders/{folder}/sync", patch(routes::mailbox::set_folder_sync))
        .route("/accounts/{id}/folders/{folder}/messages", get(routes::mailbox::list_folder_messages))
        // Mailbox
        .route("/mailbox/unified", get(routes::mailbox::unified_inbox))
        .route("/mailbox/unified/counts", get(routes::mailbox::unified_counts))
        .route("/mailbox/bulk", post(routes::mailbox::bulk_action))
        // Messages
        .route("/messages/{id}", get(routes::messages::get_message))
        .route("/messages/{id}/read", patch(routes::messages::mark_read))
        .route("/messages/{id}/flag", patch(routes::messages::toggle_flag))
        .route("/messages/{id}/archive", post(routes::messages::archive_message))
        .route("/messages/{id}", delete(routes::messages::delete_message))
        .route("/messages/{id}/move", post(routes::messages::move_message))
        .route("/messages/{id}/reanalyse", post(routes::messages::reanalyse_message))
        .route("/attachments/{id}", get(routes::messages::download_attachment))
        // Threads
        .route("/threads/{thread_id}", get(routes::threads::get_thread))
        .route("/threads/{thread_id}/archive", post(routes::threads::archive_thread))
        .route("/threads/{thread_id}/delete", post(routes::threads::delete_thread))
        .route("/threads/{thread_id}/read", patch(routes::threads::mark_thread_read))
        // Send
        .route("/send", post(routes::send::send_email))
        // Search
        .route("/search", get(routes::search::search))
        // Server autodiscovery for the account wizard
        .route("/discover", get(routes::discover::discover))
        // Contacts
        .route("/contacts", get(routes::contacts::list_contacts).post(routes::contacts::create_contact))
        .route("/contacts/{id}", delete(routes::contacts::delete_contact))
        // Calendar
        .route("/calendars", get(routes::calendar::list_calendars).post(routes::calendar::create_calendar))
        .route("/calendar/events", get(routes::calendar::list_events).post(routes::calendar::create_event))
        .route("/calendar/events/{id}", delete(routes::calendar::delete_event))
        // Rules
        .route("/rules", get(routes::rules::list_rules).post(routes::rules::create_rule))
        .route("/rules/{id}", put(routes::rules::update_rule).delete(routes::rules::delete_rule))
        .route("/accounts/{id}/apply-sieve", post(routes::rules::apply_sieve))
        // Settings
        .route("/settings", get(routes::settings::get_settings).patch(routes::settings::patch_settings))
        .route("/settings/image-allowlist", get(routes::settings::list_image_allowlist).post(routes::settings::add_image_allowlist))
        .route("/settings/image-allowlist/{sender}", delete(routes::settings::remove_image_allowlist))
        .route("/settings/phishing/reset", post(routes::settings::reset_phishing_analysis))
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

fn load_vapid_config(settings: &config::Settings) -> Option<Arc<VapidConfig>> {
    let public_key = settings.vapid_public_key.as_ref().filter(|v| !v.is_empty());
    let private_key = settings.vapid_private_key.as_ref().filter(|v| !v.is_empty());

    match (public_key, private_key) {
        (Some(public_key), Some(private_key)) => Some(Arc::new(VapidConfig {
            public_key: public_key.clone(),
            private_key: private_key.clone(),
            subject: settings.vapid_subject.clone(),
        })),
        _ => {
            tracing::warn!("web push disabled: vapid_public_key or vapid_private_key is missing");
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
///
/// Spawns a real sync task per account so background polling resumes after a
/// restart — without this, accounts sit in `pending` and never sync until a
/// manual refresh.
async fn restart_existing_accounts(state: &AppState, data_dir: &str) {
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
        let db = match state.user_db_pool.get(&user_id).await {
            Ok(db) => db,
            Err(_) => continue,
        };
        let account_ids: Vec<String> =
            sqlx::query_scalar("SELECT id FROM email_accounts")
                .fetch_all(&db)
                .await
                .unwrap_or_default();
        for account_id in account_ids {
            state
                .sync_manager
                .start_account(account_id, user_id.clone(), Arc::new(state.clone()))
                .await;
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
