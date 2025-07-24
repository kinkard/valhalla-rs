use std::{io::Write, path::PathBuf};

use miniserde::{Serialize, json};
use tempfile::NamedTempFile;

use valhalla::{Actor, Config, GraphReader};

#[derive(Serialize)]
struct ValhallaConfig {
    mjolnir: MjolnirConfig,
}

#[derive(Serialize)]
struct MjolnirConfig {
    tile_extract: String,
    traffic_extract: String,
}

const ANDORRA_CONFIG: &str = "tests/andorra/config.json";
const ANDORRA_TILES: &str = "tests/andorra/tiles.tar";
const ANDORRA_TRAFFIC: &str = "tests/andorra/traffic.tar";

#[test]
fn from_file() {
    assert!(Config::from_file(PathBuf::default()).is_err());

    // bad json
    let mut file = NamedTempFile::new().expect("Failed to create temp file for config");
    file.write_all("{{{".as_bytes())
        .expect("Failed to write config");
    assert!(Config::from_file(file.path()).is_err());

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
    // Config just ensures that the file is valid JSON. `GraphReader` will check the paths.
    let config = Config::from_file(file.path()).unwrap();
    assert!(GraphReader::new(&config).is_err());

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
    // Config just ensures that the file is valid JSON. `GraphReader` will check the paths.
    let config = Config::from_file(file.path()).unwrap();
    assert!(GraphReader::new(&config).is_ok());

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
    // Config just ensures that the file is valid JSON. `GraphReader` will check the paths.
    let config = Config::from_file(file.path()).unwrap();
    assert!(GraphReader::new(&config).is_err());

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
    // Config just ensures that the file is valid JSON. `GraphReader` will check the paths.
    let config = Config::from_file(file.path()).unwrap();
    assert!(GraphReader::new(&config).is_ok());
}

#[test]
fn from_full_config() {
    let config = Config::from_file(ANDORRA_CONFIG).unwrap();
    assert!(GraphReader::new(&config).is_ok());
    assert!(Actor::new(&config).is_ok());

    // Break the path to the tiles in the config so both GraphReader and Actor fail
    let config_json = std::fs::read_to_string(ANDORRA_CONFIG)
        .unwrap()
        .replace(ANDORRA_TILES, "bad_path_to_tile_extract");
    let config = Config::from_json(&config_json).unwrap();
    assert!(GraphReader::new(&config).is_err());
    assert!(Actor::new(&config).is_err());
}

#[test]
fn from_json() {
    assert!(Config::from_json("").is_err());
    assert!(Config::from_json("{").is_err());
    assert!(Config::from_json("}").is_err());
    assert!(Config::from_json("{}").is_ok());

    // bad config
    let config = ValhallaConfig {
        mjolnir: MjolnirConfig {
            tile_extract: "bad_path_to_tile_extract".into(),
            traffic_extract: "bad_path_to_traffic_extract".into(),
        },
    };
    let config = Config::from_json(&json::to_string(&config)).unwrap();
    assert!(GraphReader::new(&config).is_err());

    // tiles and traffic ok
    let config = ValhallaConfig {
        mjolnir: MjolnirConfig {
            tile_extract: ANDORRA_TILES.into(),
            traffic_extract: ANDORRA_TRAFFIC.into(),
        },
    };
    let config = Config::from_json(&json::to_string(&config)).unwrap();
    assert!(GraphReader::new(&config).is_ok());

    // full config
    let json = std::fs::read_to_string(ANDORRA_CONFIG).expect("Failed to read config file");
    let config = Config::from_json(&json).unwrap();
    assert!(GraphReader::new(&config).is_ok());
}

#[test]
fn from_tile_extract() {
    assert!(GraphReader::new(&Config::from_tile_extract("").unwrap()).is_err());
    assert!(
        GraphReader::new(&Config::from_tile_extract("bad_path_to_tile_extract").unwrap()).is_err()
    );
    assert!(GraphReader::new(&Config::from_tile_extract(ANDORRA_TILES).unwrap()).is_ok());
}
