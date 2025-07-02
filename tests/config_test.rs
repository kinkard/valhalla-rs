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
fn empty_config_fail() {
    assert!(GraphReader::from_file(&PathBuf::default()).is_none());
    assert!(GraphReader::from_json("").is_none());
}

#[test]
fn bad_config_fail() {
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
}

#[test]
fn tiles_only_ok() {
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
}

#[test]
fn traffic_only_fail() {
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
}

#[test]
fn tiles_and_traffic_ok() {
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
fn inline_config_ok() {
    let config = ValhallaConfig {
        mjolnir: MjolnirConfig {
            tile_extract: ANDORRA_TILES.into(),
            traffic_extract: ANDORRA_TRAFFIC.into(),
        },
    };

    assert!(GraphReader::from_json(&json::to_string(&config)).is_some());
}
