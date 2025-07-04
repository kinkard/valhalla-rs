use miniserde::{Serialize, json};
use pretty_assertions::assert_eq;

use valhalla::{GraphId, GraphLevel, GraphReader, LatLon};

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
const ANDORRA_BBOX: (LatLon, LatLon) = (LatLon(42.373627, 1.301427), LatLon(42.72199, 1.892865));

#[test]
fn tiles_in_bbox() {
    let config = ValhallaConfig {
        mjolnir: MjolnirConfig {
            tile_extract: ANDORRA_TILES.into(),
            traffic_extract: ANDORRA_TRAFFIC.into(),
        },
    };
    let reader =
        GraphReader::from_json(&json::to_string(&config)).expect("Failed to create GraphReader");

    let mut all_tiles = reader.tiles();
    all_tiles.sort_by_key(|id| id.value); // order is not guaranteed, sort for comparison
    assert!(!all_tiles.is_empty(), "Should have tiles in the dataset");

    let mut world_tiles: Vec<GraphId> =
        [GraphLevel::Highway, GraphLevel::Arterial, GraphLevel::Local]
            .iter()
            .flat_map(|&level| {
                reader.tiles_in_bbox(LatLon(-90.0, -180.0), LatLon(90.0, 180.0), level)
            })
            .collect();
    world_tiles.sort_by_key(|id| id.value);
    assert_eq!(
        all_tiles, world_tiles,
        "All tiles should equal world bbox tiles"
    );

    let mut andorra_tiles: Vec<GraphId> =
        [GraphLevel::Highway, GraphLevel::Arterial, GraphLevel::Local]
            .iter()
            .flat_map(|&level| reader.tiles_in_bbox(ANDORRA_BBOX.0, ANDORRA_BBOX.1, level))
            .collect();
    andorra_tiles.sort_by_key(|id| id.value);
    assert_eq!(
        all_tiles, andorra_tiles,
        "All tiles should equal Andorra bbox tiles"
    );

    for level in [GraphLevel::Highway, GraphLevel::Arterial, GraphLevel::Local] {
        let tiles = reader.tiles_in_bbox(ANDORRA_BBOX.0, ANDORRA_BBOX.1, level);
        assert!(!tiles.is_empty(), "No tiles found for level {level:?}");
        for tile_id in tiles {
            assert!(
                tile_id != GraphId::default(),
                "Tile ID should not be invalid"
            );
            assert_eq!(tile_id.level(), level.repr as u32);
            // GraphId::id() is the index of the edge in the tile, which is always 0 for the tile itself
            assert_eq!(tile_id.id(), 0);
        }
    }
}

#[test]
fn edges_in_tile() {
    let config = ValhallaConfig {
        mjolnir: MjolnirConfig {
            tile_extract: ANDORRA_TILES.into(),
            traffic_extract: ANDORRA_TRAFFIC.into(),
        },
    };
    let reader =
        GraphReader::from_json(&json::to_string(&config)).expect("Failed to create GraphReader");

    assert!(
        reader.get_tile(GraphId::default()).is_none(),
        "Default tile should not exist"
    );

    for tile_id in reader.tiles() {
        let tile = reader.get_tile(tile_id);
        assert!(tile.is_some(), "Tile should exist for ID: {tile_id:?}");
        let tile = tile.unwrap();
        assert_eq!(tile.id(), tile_id, "Tile ID mismatch for {tile_id:?}");

        let slice = tile.directededges();
        assert!(!slice.is_empty(), "Tile should always have directed edges");
        for (i, de) in slice.iter().enumerate() {
            // Ensure that the directed edge index matches the slice index.
            // This assertion ensures that the pointer arithmetic in the Rust FFI is correct.
            let via_ptr = tile.directededge(i).unwrap();
            assert_eq!(
                de as *const _, via_ptr as *const _,
                "de and via_ptr should have the same address"
            );

            // this tileset has no historical traffic data
            assert_eq!(de.free_flow_speed(), 0);
            assert_eq!(de.constrained_flow_speed(), 0);
            assert_ne!(de.speed(), 0, "Default edge's speed should never be zero");

            let ei = tile.edgeinfo(de);
            if de.is_shortcut() {
                assert_eq!(ei.way_id, 0, "All shortcuts should have way_id 0");
            } else {
                assert_ne!(ei.way_id, 0, "Non-shortcuts should have valid way_id");
            }
        }
        assert!(tile.directededge(slice.len()).is_none());
    }
}
