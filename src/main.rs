use std::{collections::HashMap, env, num::NonZero, time::Instant};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Html,
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use libvalhalla::{GraphLevel, LatLon};
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
    /// Valhalla base url to send requests to
    #[arg(long, default_value = "http://localhost:8002")]
    valhalla_url: String,
    /// Path to valhalla json config file.
    /// Required for an access to valhalla graph information.
    #[arg(long)]
    valhalla_config_path: Option<String>,
}

#[derive(Clone)]
struct AppState {
    http_client: reqwest::Client,
    valhalla_url: String,
    graph_reader: Option<libvalhalla::GraphReader>,
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
        .route("/api/traffic/:bbox", get(traffic))
        .with_state(AppState {
            http_client: reqwest::Client::new(),
            valhalla_url: config.valhalla_url,
            graph_reader: config
                .valhalla_config_path
                .map(|path| libvalhalla::GraphReader::new(path.into())),
        });

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
    let begin = Instant::now();
    let response = state
        .http_client
        .post(format!("{}/{}", state.valhalla_url, request.endpoint))
        .json(&request.payload)
        .send()
        .await
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
    info!(
        "Fetched /{} in {}ms",
        request.endpoint,
        begin.elapsed().as_millis()
    );

    response
        .json()
        .await
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))
}

async fn traffic(
    State(state): State<AppState>,
    Path(bbox): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let Some(bbox) = parse_bbox(&bbox) else {
        return Err((
            StatusCode::BAD_REQUEST,
            "Bad bbox, expecting 'min_lat,min_lon;max_lat,max_lon'".to_string(),
        ));
    };

    let Some(reader) = &state.graph_reader else {
        return Err((
            StatusCode::IM_A_TEAPOT,
            "Traffic information was not enabled".to_string(),
        ));
    };

    let begin = Instant::now();
    let edges = [GraphLevel::Highway, GraphLevel::Arterial, GraphLevel::Local]
        .into_iter()
        .map(|level| reader.tiles_in_bbox(bbox.0, bbox.1, level))
        // Limit number of traffic tiles we fetch
        .scan(0, |count, tiles| {
            *count += tiles.len();
            if *count < 20 {
                Some(tiles)
            } else {
                None
            }
        })
        .flatten()
        .flat_map(|tile_id| {
            // todo: this is really heavy compute operation
            reader.get_tile_traffic_flows(tile_id)
        })
        .map(|edge| (edge.shape, 10 - (edge.jam_factor * 10.0).round() as i32))
        .collect::<HashMap<_, _>>();
    info!(
        "Fetched {} traffic edges in {}ms",
        edges.len(),
        begin.elapsed().as_millis()
    );
    Ok(Json(serde_json::to_value(edges).unwrap()))
}

fn parse_coordinate(coord: &str) -> Option<LatLon> {
    let (lat, lon) = coord.split_once(',')?;
    let lat = lat.parse::<f32>().ok()?;
    let lon = lon.parse::<f32>().ok()?;
    Some(LatLon(lat, lon))
}

fn parse_bbox(bbox: &str) -> Option<(LatLon, LatLon)> {
    let (min, max) = bbox.split_once(';')?;
    let min = parse_coordinate(min)?;
    let max = parse_coordinate(max)?;
    Some((min, max))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bbox() {
        assert_eq!(
            parse_bbox("55.0,13.0;56.0,14.0"),
            Some((LatLon(55.0, 13.0), LatLon(56.0, 14.0)))
        );
        assert_eq!(
            parse_bbox("37.7749,-122.4194;34.0522,-118.2437"),
            Some((LatLon(37.7749, -122.4194), LatLon(34.0522, -118.2437)))
        );

        // missing semicolon
        assert_eq!(parse_bbox("37.7749,-122.4194 34.0522,-118.2437"), None);
        // missing comma
        assert_eq!(parse_bbox("37.7749 -122.4194;34.0522,-118.2437"), None);
        assert_eq!(parse_bbox("37.7749;-122.4194;34.0522,-118.2437"), None);
        assert_eq!(parse_bbox("37.7749-122.4194;34.0522,-118.2437"), None);
        assert_eq!(parse_bbox("37.7749,-122.4194;34.0522 -118.2437"), None);
        assert_eq!(parse_bbox("37.7749,-122.4194;34.0522;-118.2437"), None);
        assert_eq!(parse_bbox("37.7749,-122.4194;34.0522-118.2437"), None);
        // not a number
        assert_eq!(parse_bbox("invalid;34.0522,-118.2437"), None);
        assert_eq!(parse_bbox("37.7749,invalid;34.0522,-118.2437"), None);
        assert_eq!(parse_bbox("37.7749,-122.4194;invalid,-118.2437"), None);
        assert_eq!(parse_bbox("37.7749,-122.4194;34.0522,invalid"), None);
    }
}
