use std::io::Write;

use miniserde::{Serialize, json};
use pretty_assertions::assert_eq;
use tempfile::NamedTempFile;

use valhalla::{GraphLevel, GraphReader, LatLon};

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
    let reader = GraphReader::new(file.path()).expect("Failed to create GraphReader");

    let world = (LatLon(-90.0, -180.0), LatLon(90.0, 180.0));
    let tiles = reader.tiles_in_bbox(world.0, world.1, GraphLevel::Highway);
    assert_eq!(tiles.len(), 1);
    assert!(
        tiles
            .iter()
            .all(|t| t.level() == GraphLevel::Highway.repr as u32)
    );
    let tiles = reader.tiles_in_bbox(world.0, world.1, GraphLevel::Arterial);
    assert_eq!(tiles.len(), 1);
    assert!(
        tiles
            .iter()
            .all(|t| t.level() == GraphLevel::Arterial.repr as u32)
    );
    let tiles = reader.tiles_in_bbox(world.0, world.1, GraphLevel::Local);
    assert_eq!(tiles.len(), 5);
    assert!(
        tiles
            .iter()
            .all(|t| t.level() == GraphLevel::Local.repr as u32)
    );
}
