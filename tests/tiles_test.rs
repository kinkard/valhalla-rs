use miniserde::{Serialize, json};
use pretty_assertions::assert_eq;

use valhalla::{Config, GraphId, GraphLevel, GraphReader, LatLon, LiveTraffic, TimeZoneInfo};

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

    let tile = reader.graph_tile(reader.tiles()[0]).unwrap();
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
        reader.graph_tile(GraphId::default()).is_none(),
        "Default tile should not exist"
    );
    assert!(
        reader.traffic_tile(GraphId::default()).is_none(),
        "Default traffic tile should not exist"
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

            let tile = reader.graph_tile(tile_id);
            assert!(tile.is_some(), "Tile should exist for ID: {tile_id:?}");
            let tile = tile.unwrap();
            assert_eq!(tile.id(), tile_id, "Tile ID mismatch for {tile_id:?}");

            let traffic_tile = reader.traffic_tile(tile_id);
            assert!(
                traffic_tile.is_some(),
                "Traffic tile should exist for ID: {tile_id:?}"
            );
            let traffic_tile = traffic_tile.unwrap();
            assert_eq!(
                traffic_tile.id(),
                tile_id,
                "Traffic tile ID mismatch for {tile_id:?}"
            );
            assert_eq!(
                traffic_tile.edge_count() as usize,
                tile.directededges().len(),
                "Mismatch in edge count for {tile_id:?}"
            );
        }
    }

    let tile = reader.graph_tile(reader.tiles()[0]).unwrap();
    // The first admin info is always empty (for ocean)
    let admininfo = tile.admin_info(0).unwrap();
    assert_eq!(admininfo.country_iso, "");
    assert_eq!(admininfo.state_iso, "");
    assert_eq!(admininfo.country_text, "None");
    assert_eq!(admininfo.state_text, "None");

    // The second admin info is Andorra
    let admininfo = tile.admin_info(1).unwrap();
    assert_eq!(admininfo.country_iso, "AD");
    assert_eq!(admininfo.state_iso, "");
    assert_eq!(admininfo.country_text, "Andorra");
    assert_eq!(admininfo.state_text, "");

    // There is no third admin info in this tileset
    assert!(tile.admin_info(2).is_none());
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
        let tile = reader.graph_tile(tile_id).unwrap();

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
                assert!(reader.graph_tile(endnode.tile()).is_some());
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
        let tile = reader.graph_tile(tile_id).unwrap();

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

            // Check no panic
            let _ = tile.node_edges(node);
            let _ = tile.node_transitions(node);

            assert_eq!(node.admin_index(), 1); // Andorra only, see [`tiles_in_bbox`] test

            // Europe/Andorra or Europe/Madrid or Europe/Paris timezones
            assert!(matches!(node.timezone(), 293 | 313 | 319));
        }
    }
}

#[test]
fn reverse_edge() {
    let config = ValhallaConfig {
        mjolnir: MjolnirConfig {
            tile_extract: ANDORRA_TILES.into(),
            traffic_extract: ANDORRA_TRAFFIC.into(),
        },
    };
    let reader = GraphReader::new(&Config::from_json(&json::to_string(&config)).unwrap())
        .expect("Failed to create GraphReader");

    let tile_id = reader.tiles()[0];
    let tile = reader.graph_tile(tile_id).unwrap();

    for (de_index, de) in tile.directededges().iter().enumerate() {
        if de.leaves_tile() || de.is_shortcut() {
            // just don't bother with such edges for this test
            continue;
        }

        let end_node = tile.node(de.endnode().id()).unwrap();
        let opp_de = &tile.node_edges(end_node)[de.opp_index() as usize];
        assert_eq!(tile.edgeinfo(de).way_id, tile.edgeinfo(opp_de).way_id);

        let begin_node = tile.node(opp_de.endnode().id()).unwrap();
        assert_eq!(
            de_index,
            (begin_node.edge_index() + opp_de.opp_index()) as usize
        );
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
        let tile = reader.graph_tile(tile_id).unwrap();

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
fn live_traffic() {
    // for this test we should work with copy of the traffic tar to avoid modifying the original one
    let traffic_copy = "tests/andorra/traffic_copy.tar";
    std::fs::copy(ANDORRA_TRAFFIC, traffic_copy).expect("Failed to copy traffic tar");
    // Poor man's `defer`
    struct Cleanup<'a>(&'a str);
    impl<'a> Drop for Cleanup<'a> {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.0);
        }
    }
    let _cleanup = Cleanup(traffic_copy);

    let config = ValhallaConfig {
        mjolnir: MjolnirConfig {
            tile_extract: ANDORRA_TILES.into(),
            traffic_extract: traffic_copy.into(),
        },
    };

    let reader = GraphReader::new(&Config::from_json(&json::to_string(&config)).unwrap())
        .expect("Failed to create GraphReader");
    let tile_id = reader.tiles()[0];
    let tile = reader.graph_tile(tile_id).unwrap();
    let traffic_tile = reader.traffic_tile(tile_id).unwrap();

    assert_eq!(traffic_tile.last_update(), 0); // initial state
    traffic_tile.write_last_update(101);
    assert_eq!(traffic_tile.last_update(), 101);
    traffic_tile.write_spare(999);
    assert_eq!(traffic_tile.spare(), 999);
    traffic_tile.clear_traffic(); // it's important to reset it back for other tests
    assert_eq!(traffic_tile.last_update(), 0);
    assert_eq!(traffic_tile.spare(), 999); // spare is not cleared
    traffic_tile.write_spare(0); // reset spare too

    assert_ne!(traffic_tile.edge_count(), 0);
    let edge_id = 0;
    let edge = tile.directededge(edge_id).unwrap();

    // no traffic data in the tileset
    assert_eq!(
        traffic_tile.edge_traffic(edge_id),
        Some(LiveTraffic::UNKNOWN)
    );
    assert_eq!(tile.live_speed(edge), None);

    traffic_tile.write_edge_traffic(edge_id, LiveTraffic::CLOSED);
    assert_eq!(tile.live_speed(edge), Some(0));

    traffic_tile.write_edge_traffic(edge_id, LiveTraffic::from_uniform_speed(72));
    assert_eq!(tile.live_speed(edge), Some(72));

    // speed is stored with 2km/h precision
    traffic_tile.write_edge_traffic(edge_id, LiveTraffic::from_uniform_speed(73));
    assert_eq!(tile.live_speed(edge), Some(72));

    // only "overall speed" is used by `live_speed()`. Segmented speeds though are accessible via `/locate`
    traffic_tile.write_edge_traffic(
        edge_id,
        LiveTraffic::from_segmented_speeds(72, [1, 2, 3], [127, 128]),
    );
    assert_eq!(tile.live_speed(edge), Some(72));
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

#[test]
#[should_panic = "Wrong tile"]
fn wrong_tile_edgeinfo() {
    let reader = GraphReader::new(&Config::from_tile_extract(ANDORRA_TILES).unwrap())
        .expect("Failed to create GraphReader");

    let tiles = reader.tiles();
    assert!(tiles.len() >= 2, "This test requires at least two tiles");
    let t1 = reader.graph_tile(tiles[0]).unwrap();
    let t2 = reader.graph_tile(tiles[1]).unwrap();

    let de = t2.directededge(0).unwrap();
    let _ = t1.edgeinfo(de); // should panic
}

#[test]
#[should_panic = "Wrong tile"]
fn wrong_tile_node_edges() {
    let reader = GraphReader::new(&Config::from_tile_extract(ANDORRA_TILES).unwrap())
        .expect("Failed to create GraphReader");

    let tiles = reader.tiles();
    assert!(tiles.len() >= 2, "This test requires at least two tiles");
    let t1 = reader.graph_tile(tiles[0]).unwrap();
    let t2 = reader.graph_tile(tiles[1]).unwrap();

    let node = t2.node(0).unwrap();
    let _ = t1.node_edges(node); // should panic
}

#[test]
#[should_panic = "Wrong tile"]
fn wrong_tile_node_transitions() {
    let reader = GraphReader::new(&Config::from_tile_extract(ANDORRA_TILES).unwrap())
        .expect("Failed to create GraphReader");

    let tiles = reader.tiles();
    assert!(tiles.len() >= 2, "This test requires at least two tiles");
    let t1 = reader.graph_tile(tiles[0]).unwrap();
    let t2 = reader.graph_tile(tiles[1]).unwrap();

    let node = t2.node(0).unwrap();
    let _ = t1.node_transitions(node); // should panic
}
