use miniserde::{Serialize, json};
use pretty_assertions::assert_eq;

use valhalla::{Config, GraphId, GraphLevel, GraphReader, LatLon, TimeZoneInfo};

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
fn dataset_id() {
    let config = ValhallaConfig {
        mjolnir: MjolnirConfig {
            tile_extract: ANDORRA_TILES.into(),
            traffic_extract: ANDORRA_TRAFFIC.into(),
        },
    };
    let reader = GraphReader::new(&Config::from_json(&json::to_string(&config)).unwrap())
        .expect("Failed to create GraphReader");

    assert_eq!(reader.dataset_id(), 12953172102);
}

#[test]
fn tile_can_outlive_reader() {
    let config = ValhallaConfig {
        mjolnir: MjolnirConfig {
            tile_extract: ANDORRA_TILES.into(),
            traffic_extract: ANDORRA_TRAFFIC.into(),
        },
    };
    let reader = GraphReader::new(&Config::from_json(&json::to_string(&config)).unwrap())
        .expect("Failed to create GraphReader");

    let tile = reader.tile(reader.tiles()[0]).unwrap();
    // just do something complicated with the tile
    let count = tile
        .directededges()
        .iter()
        .filter(|e| tile.edgeinfo(e).speed_limit == 0)
        .count();
    assert_ne!(count, 0);

    // Dropping reader should not affect the tile and its data
    drop(reader);

    let other_count = tile
        .directededges()
        .iter()
        .filter(|e| tile.edgeinfo(e).speed_limit == 0)
        .count();
    assert_eq!(
        count, other_count,
        "Tile should remain valid after reader is dropped"
    );
}

#[test]
fn tiles_in_bbox() {
    let config = ValhallaConfig {
        mjolnir: MjolnirConfig {
            tile_extract: ANDORRA_TILES.into(),
            traffic_extract: ANDORRA_TRAFFIC.into(),
        },
    };
    let reader = GraphReader::new(&Config::from_json(&json::to_string(&config)).unwrap())
        .expect("Failed to create GraphReader");

    assert!(
        reader.tile(GraphId::default()).is_none(),
        "Default tile should not exist"
    );

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
            assert_eq!(tile_id.tile(), tile_id);

            let tile = reader.tile(tile_id);
            assert!(tile.is_some(), "Tile should exist for ID: {tile_id:?}");
            let tile = tile.unwrap();
            assert_eq!(tile.id(), tile_id, "Tile ID mismatch for {tile_id:?}");
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
    let reader = GraphReader::new(&Config::from_json(&json::to_string(&config)).unwrap())
        .expect("Failed to create GraphReader");

    for tile_id in reader.tiles() {
        let tile = reader.tile(tile_id).unwrap();

        let slice = tile.directededges();
        assert!(!slice.is_empty(), "Tile should always have directed edges");
        for (i, de) in slice.iter().enumerate() {
            // Ensure that the directed edge index matches the slice index.
            // This assertion ensures that the pointer arithmetic in the Rust FFI is correct.
            let via_ptr = tile.directededge(i as u32).unwrap();
            assert_eq!(
                de as *const _, via_ptr as *const _,
                "de and via_ptr should have the same address"
            );

            // this tileset has no historical traffic data
            assert_eq!(de.free_flow_speed(), 0);
            assert_eq!(de.constrained_flow_speed(), 0);
            assert_ne!(de.speed(), 0, "Default edge's speed should never be zero");
            assert_eq!(tile.live_speed(de), None);
            assert_eq!(tile.edge_closed(de), false);
            assert_eq!(
                tile.edge_speed(de, valhalla::SpeedSources::ALL, false, 0, 0),
                (de.speed(), valhalla::SpeedSources::NO_FLOW)
            );
            let truck_speed = if de.truck_speed() > 0 {
                de.truck_speed()
            } else {
                de.speed()
            };
            assert_eq!(
                tile.edge_speed(de, valhalla::SpeedSources::ALL, true, 0, 0),
                (truck_speed, valhalla::SpeedSources::NO_FLOW)
            );

            let ei = tile.edgeinfo(de);
            assert_eq!(de.is_shortcut(), ei.way_id == 0, "Shortcuts have way_id 0");

            let endnode = de.endnode();
            assert_eq!(de.leaves_tile(), de.endnode().tile() != tile_id.tile());
            if de.leaves_tile() {
                assert!(reader.tile(endnode.tile()).is_some());
            }
        }
        assert!(tile.directededge(slice.len() as u32).is_none());
    }
}

#[test]
fn nodes_in_tile() {
    let config = ValhallaConfig {
        mjolnir: MjolnirConfig {
            tile_extract: ANDORRA_TILES.into(),
            traffic_extract: ANDORRA_TRAFFIC.into(),
        },
    };
    let reader = GraphReader::new(&Config::from_json(&json::to_string(&config)).unwrap())
        .expect("Failed to create GraphReader");

    for tile_id in reader.tiles() {
        let tile = reader.tile(tile_id).unwrap();

        let tile_edges = tile.directededges();
        let tile_transitions = tile.transitions();

        // Same check for nodes
        let slice = tile.nodes();
        assert!(!slice.is_empty(), "Tile should always have nodes");
        for (i, node) in slice.iter().enumerate() {
            // Ensure that the node index matches the slice index.
            // This assertion ensures that the pointer arithmetic in the Rust FFI is correct.
            let via_ptr = tile.node(i as u32).unwrap();
            assert_eq!(
                node as *const _, via_ptr as *const _,
                "node and via_ptr should have the same address"
            );

            assert!(tile_edges.get(node.edges()).is_some());
            assert!(tile_transitions.get(node.transitions()).is_some());

            // Europe/Andorra or Europe/Madrid or Europe/Paris timezones
            assert!(matches!(node.timezone(), 293 | 313 | 319));
        }
    }
}

#[test]
fn transitions_in_tile() {
    let config = ValhallaConfig {
        mjolnir: MjolnirConfig {
            tile_extract: ANDORRA_TILES.into(),
            traffic_extract: ANDORRA_TRAFFIC.into(),
        },
    };
    let reader = GraphReader::new(&Config::from_json(&json::to_string(&config)).unwrap())
        .expect("Failed to create GraphReader");

    let mut transition_count = 0;
    for tile_id in reader.tiles() {
        let tile = reader.tile(tile_id).unwrap();

        // Same check for transitions
        for (i, transition) in tile.transitions().iter().enumerate() {
            transition_count += 1;

            // Ensure that the node index matches the slice index.
            // This assertion ensures that the pointer arithmetic in the Rust FFI is correct.
            let via_ptr = tile.transition(i as u32).unwrap();
            assert_eq!(
                transition as *const _, via_ptr as *const _,
                "transition and via_ptr should have the same address"
            );

            assert_ne!(
                transition.endnode().tile(),
                tile_id.tile(),
                "Transition endnode should be in a different tile"
            );
            assert_eq!(
                transition.upward(),
                transition.endnode().level() < tile_id.level()
            );
        }
    }
    assert_eq!(transition_count, 3550); // to be changed if tileset changes
}

#[test]
fn tz_info() {
    // Summer
    let unix_timestamp = 1750000000; // Jun 15 2025
    assert!(TimeZoneInfo::from_id(0, unix_timestamp).is_none());

    let tz = TimeZoneInfo::from_id(293, unix_timestamp).unwrap();
    assert_eq!(tz.name, "Europe/Andorra");
    assert_eq!(tz.offset_seconds, 7200); // UTC+2

    let tz = TimeZoneInfo::from_id(94, unix_timestamp).unwrap();
    assert_eq!(tz.name, "America/Los_Angeles");
    assert_eq!(tz.offset_seconds, -25200); // UTC-7

    // Winter
    let unix_timestamp = 1740000000; // Feb 19 2025
    assert!(TimeZoneInfo::from_id(0, unix_timestamp).is_none());

    let tz = TimeZoneInfo::from_id(293, unix_timestamp).unwrap();
    assert_eq!(tz.name, "Europe/Andorra");
    assert_eq!(tz.offset_seconds, 3600); // UTC+1

    let tz = TimeZoneInfo::from_id(94, unix_timestamp).unwrap();
    assert_eq!(tz.name, "America/Los_Angeles");
    assert_eq!(tz.offset_seconds, -28800); // UTC-8
}
