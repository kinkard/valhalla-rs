use std::{env, num::NonZero};

use axum::{
    extract::State,
    http::StatusCode,
    response::Html,
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use serde::Deserialize;
use serde_json::Value;
use tokio::{fs::File, io::AsyncReadExt, signal};
use tracing::info;

#[derive(Parser)]
struct Config {
    /// Port to listen
    #[arg(long, default_value_t = 3000)]
    port: u16,
    /// Max threads to use
    #[arg(long, default_value_t = 4)]
    concurrency: u16,
}

#[derive(Clone, Default)]
struct AppState {
    http_client: reqwest::Client,
}

fn main() {
    tracing_subscriber::fmt::init();

    let config = Config::parse();

    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(
            std::thread::available_parallelism()
                .map(NonZero::get)
                .unwrap_or(16) // fallback to 16 as max if we can't get the number of CPUs
                .min(config.concurrency as usize),
        )
        .enable_all()
        .build()
        .unwrap()
        .block_on(run(config))
}

async fn run(config: Config) {
    // build our application with a route
    let app = Router::new()
        .route("/", get(serve_index_html))
        .route("/api/request", post(forward_request))
        .with_state(AppState::default());

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port))
        .await
        .unwrap();
    info!("Listening at http://localhost:{}", config.port);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::select! {
                _ = signal::ctrl_c() => {
                    info!("Ctrl+C received, shutting down");
                }
                _ = async {
                    signal::unix::signal(signal::unix::SignalKind::terminate())
                        .expect("failed to install SIGTERM signal handler")
                        .recv()
                        .await
                } => {
                    info!("SIGTERM received, shutting down");
                }
            }
        })
        .await
        .unwrap();
}

async fn serve_index_html() -> Result<Html<String>, (StatusCode, String)> {
    let index_html = "web/index.html";
    let Ok(mut file) = File::open(index_html).await else {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Failed to open {index_html}: not found"),
        ));
    };

    let mut contents = String::new();
    if let Err(err) = file.read_to_string(&mut contents).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read {index_html}: {err}"),
        ));
    }

    let access_token = env::var("MAPBOX_ACCESS_TOKEN").unwrap_or_default();
    let contents = contents.replace("{{MAPBOX_ACCESS_TOKEN}}", &access_token);

    Ok(Html(contents))
}

#[derive(Deserialize)]
struct RequestToForward {
    /// Valhalla API endpoint. See https://valhalla.github.io/valhalla/api for more details.
    endpoint: String,
    /// Data to send
    payload: Value,
}

async fn forward_request(
    State(state): State<AppState>,
    Json(request): Json<RequestToForward>,
) -> Result<Json<Value>, (StatusCode, String)> {
    info!("Requested /{}", request.endpoint);
    let response = state
        .http_client
        .post(format!("http://localhost:8002/{}", request.endpoint))
        .json(&request.payload)
        .send()
        .await
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    response
        .json()
        .await
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))
}
