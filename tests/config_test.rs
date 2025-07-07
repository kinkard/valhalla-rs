use std::{io::Write, path::PathBuf};

use miniserde::{Serialize, json};
use tempfile::NamedTempFile;

use valhalla::GraphReader;

#[derive(Serialize)]
struct ValhallaConfig {
    mjolnir: MjolnirConfig,
}

#[derive(Serialize)]
struct MjolnirConfig {
    tile_extract: String,
    traffic_extract: String,
}

const ANDORRA_TILES: &str = "tests/andorra/tiles.tar";
const ANDORRA_TRAFFIC: &str = "tests/andorra/traffic.tar";

#[test]
fn from_file() {
    assert!(GraphReader::from_file(PathBuf::default()).is_none());

    // bad config
    let config = ValhallaConfig {
        mjolnir: MjolnirConfig {
            tile_extract: "bad_path_to_tile_extract".into(),
            traffic_extract: "bad_path_to_traffic_extract".into(),
        },
    };
    let mut file = NamedTempFile::new().expect("Failed to create temp file for config");
    file.write_all(json::to_string(&config).as_bytes())
        .expect("Failed to write config");
    assert!(GraphReader::from_file(file.path()).is_none());

    // tiles only
    let config = ValhallaConfig {
        mjolnir: MjolnirConfig {
            tile_extract: ANDORRA_TILES.into(),
            traffic_extract: "bad_path_to_traffic_extract".into(), // Bad traffic extract
        },
    };
    let mut file = NamedTempFile::new().expect("Failed to create temp file for config");
    file.write_all(json::to_string(&config).as_bytes())
        .expect("Failed to write config");
    assert!(GraphReader::from_file(file.path()).is_some());

    // traffic only
    let config = ValhallaConfig {
        mjolnir: MjolnirConfig {
            tile_extract: "bad_path_to_tile_extract".into(), // No tile extract
            traffic_extract: ANDORRA_TRAFFIC.into(),
        },
    };
    let mut file = NamedTempFile::new().expect("Failed to create temp file for config");
    file.write_all(json::to_string(&config).as_bytes())
        .expect("Failed to write config");
    assert!(GraphReader::from_file(file.path()).is_none());

    // tiles and traffic
    let config = ValhallaConfig {
        mjolnir: MjolnirConfig {
            tile_extract: ANDORRA_TILES.into(),
            traffic_extract: ANDORRA_TRAFFIC.into(),
        },
    };
    let mut file = NamedTempFile::new().expect("Failed to create temp file for config");
    file.write_all(json::to_string(&config).as_bytes())
        .expect("Failed to write config");
    assert!(GraphReader::from_file(file.path()).is_some());
}

#[test]
fn from_json() {
    assert!(GraphReader::from_json("").is_none());
    assert!(GraphReader::from_json("{}").is_none());

    // bad config
    let config = ValhallaConfig {
        mjolnir: MjolnirConfig {
            tile_extract: "bad_path_to_tile_extract".into(),
            traffic_extract: "bad_path_to_traffic_extract".into(),
        },
    };
    assert!(GraphReader::from_json(&json::to_string(&config)).is_none());

    // tiles and traffic ok
    let config = ValhallaConfig {
        mjolnir: MjolnirConfig {
            tile_extract: ANDORRA_TILES.into(),
            traffic_extract: ANDORRA_TRAFFIC.into(),
        },
    };
    assert!(GraphReader::from_json(&json::to_string(&config)).is_some());
}

#[test]
fn from_tiles() {
    assert!(GraphReader::from_tile_extract("").is_none());
    assert!(GraphReader::from_tile_extract("bad_path_to_tile_extract").is_none());
    assert!(GraphReader::from_tile_extract(ANDORRA_TILES).is_some());
}
