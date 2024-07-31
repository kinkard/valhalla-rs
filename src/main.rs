use std::env;

use axum::{
    extract::State,
    http::StatusCode,
    response::Html,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use tokio::{fs::File, io::AsyncReadExt};
use tracing::info;

#[derive(Clone, Default)]
struct AppState {
    http_client: reqwest::Client,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // build our application with a route
    let app = Router::new()
        .route("/", get(serve_index_html))
        .route("/api/request", post(forward_request))
        .with_state(AppState::default());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    info!("Listening at http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
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
    url: String,
    payload: Value,
}

async fn forward_request(
    State(state): State<AppState>,
    Json(request): Json<RequestToForward>,
) -> Result<Json<Value>, (StatusCode, String)> {
    info!("Requested {}", request.url);
    let response = state
        .http_client
        .post(request.url)
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
