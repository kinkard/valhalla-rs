//! `valhalla::breaking`, run against the same workloads the examples do.

use std::path::Path;

use valhalla::breaking::{TileArchive, TileId, tile_file_suffix, tile_id_from_path};
use valhalla::{GraphId, GraphLevel, LatLon};

const ANDORRA_TILES: &str = "tests/andorra/tiles.tar";
const ANDORRA_TRAFFIC: &str = "tests/andorra/traffic.tar";
const ANDORRA_BBOX: (LatLon, LatLon) = (LatLon(42.373627, 1.301427), LatLon(42.72199, 1.892865));

/// `tiles.tar` unpacked into a `tile_dir` layout, built once and shared by every test that needs
/// a directory backend.
fn andorra_tile_dir() -> &'static Path {
    use std::sync::OnceLock;
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let status = std::process::Command::new("tar")
            .arg("-xf")
            .arg(std::fs::canonicalize(ANDORRA_TILES).unwrap())
            .current_dir(dir.path())
            .status()
            .expect("tar");
        assert!(status.success(), "failed to unpack {ANDORRA_TILES}");
        dir
    })
    .path()
}

// ---------------------------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------------------------

#[test]
fn archive_reads_graph_tiles() {
    let graph = TileArchive::open_graph(ANDORRA_TILES).unwrap();
    assert_eq!(graph.tiles().len(), 7);

    let id = graph.tiles()[0];
    let tile = graph.graph_tile(id).expect("tile");
    assert_eq!(TileId::of(tile.id()), id);
    assert!(!tile.directededges().is_empty());
}

#[test]
fn archive_reads_traffic_tiles() {
    let traffic = TileArchive::open_traffic(ANDORRA_TRAFFIC).unwrap();
    assert_eq!(traffic.tiles().len(), 7);

    let id = traffic.tiles()[0];
    let tile = traffic.traffic_tile(id).expect("traffic tile");
    assert!(tile.edge_count() > 0);
}

#[test]
fn tile_id_roundtrips_through_a_path() {
    let graph = TileArchive::open_graph(ANDORRA_TILES).unwrap();
    for id in graph.tiles() {
        let path = tile_file_suffix(id, false);
        assert_eq!(tile_id_from_path(&path).unwrap(), id, "{path}");
    }
}

#[test]
fn tile_id_is_four_bytes_and_keeps_level() {
    assert_eq!(size_of::<TileId>(), 4);
    assert_eq!(size_of::<Option<TileId>>(), 8);

    let id = GraphId::from_parts(2, 519120, 1234).unwrap();
    let tile = TileId::of(id);
    assert_eq!(tile.level().repr, 2);
    assert_eq!(tile.tileid(), 519120);
    // The id bits are gone, which is the whole point.
    assert_eq!(tile.graph_id(), id.tile());
}

#[test]
fn graph_tile_from_a_directory() {
    let dir = andorra_tile_dir().display().to_string();
    let graph = TileArchive::open_graph(ANDORRA_TILES).unwrap();

    for id in graph.tiles() {
        let from_tar = graph.graph_tile(id).expect("tar tile");
        let from_dir = valhalla::breaking::graph_tile_from_dir(&dir, id).expect("dir tile");
        assert_eq!(from_tar.id(), from_dir.id());
        assert_eq!(
            from_tar.directededges().len(),
            from_dir.directededges().len()
        );
        assert_eq!(from_tar.nodes().len(), from_dir.nodes().len());
    }
}

/// The mmap directory path reads the same tiles as the read-into-vector one, and as the extract.
#[test]
fn graph_tile_from_a_directory_via_mmap() {
    let dir = andorra_tile_dir().display().to_string();
    let graph = TileArchive::open_graph(ANDORRA_TILES).unwrap();

    for id in graph.tiles() {
        let from_tar = graph.graph_tile(id).expect("tar tile");
        let mapped = valhalla::breaking::graph_tile_from_dir_mmap(&dir, id).expect("mmap tile");
        assert_eq!(mapped.id(), from_tar.id());
        assert_eq!(mapped.directededges().len(), from_tar.directededges().len());
        assert_eq!(mapped.nodes().len(), from_tar.nodes().len());

        // Edge info decodes identically, so the mapping is not just structurally right.
        let de = &mapped.directededges()[0];
        assert_eq!(
            mapped.edgeinfo(de).way_id,
            from_tar.edgeinfo(&from_tar.directededges()[0]).way_id
        );
    }

    assert!(
        valhalla::breaking::graph_tile_from_dir_mmap(&dir, TileId::new(GraphLevel::Highway, 1))
            .is_none()
    );
}

#[test]
fn graph_tile_from_bytes() {
    let dir = andorra_tile_dir();
    let graph = TileArchive::open_graph(ANDORRA_TILES).unwrap();

    for id in graph.tiles() {
        let bytes = std::fs::read(dir.join(tile_file_suffix(id, false))).expect("tile file");
        let from_bytes = valhalla::breaking::graph_tile_from_bytes(id, &bytes).expect("bytes tile");
        let from_tar = graph.graph_tile(id).expect("tar tile");
        assert_eq!(from_bytes.id(), from_tar.id());
        assert_eq!(
            from_bytes.directededges().len(),
            from_tar.directededges().len()
        );
    }
}

#[test]
fn a_missing_tile_is_none_everywhere() {
    let absent = TileId::new(GraphLevel::Highway, 1);
    let graph = TileArchive::open_graph(ANDORRA_TILES).unwrap();
    let dir = andorra_tile_dir().display().to_string();

    assert!(graph.graph_tile(absent).is_none());
    assert!(valhalla::breaking::graph_tile_from_dir(&dir, absent).is_none());
    assert!(valhalla::breaking::graph_tile_from_bytes(absent, b"not a tile").is_none());
}

/// Pure geometry: it covers every local tile the extract holds, and over the whole world it names
/// every cell of the 0.25 degree grid whether or not anything was built there.
#[test]
fn tile_id_covering_is_geometry() {
    let graph = TileArchive::open_graph(ANDORRA_TILES).unwrap();

    let candidates = TileId::covering(ANDORRA_BBOX.0, ANDORRA_BBOX.1, GraphLevel::Local);
    for id in graph.tiles() {
        if id.level().repr == GraphLevel::Local.repr {
            assert!(candidates.contains(&id), "{id:?} not covered");
        }
    }

    let world = TileId::covering(
        LatLon(-90.0, -180.0),
        LatLon(90.0, 180.0),
        GraphLevel::Local,
    );
    assert_eq!(world.len(), 1440 * 720);
}

/// A writable copy of the traffic fixture, so a test can prove a join is live by writing through
/// it. Never touches `tests/andorra/traffic.tar`.
fn writable_traffic_copy() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("traffic.tar");
    std::fs::copy(ANDORRA_TRAFFIC, &path).expect("copy traffic.tar");
    let path = path.display().to_string();
    (dir, path)
}

/// The combination C++ supports and today's `GraphReader` cannot express: graph tiles off disk,
/// live traffic out of a tar.
///
/// Proved by writing a speed through the traffic archive and reading it back through a
/// *directory-backed* graph tile - so the join is real, not just structurally present.
#[test]
fn a_directory_backend_can_carry_traffic() {
    let dir = andorra_tile_dir().display().to_string();
    let (_tmp, traffic_path) = writable_traffic_copy();
    let traffic = TileArchive::open_traffic_writable(&traffic_path).unwrap();
    let graph = TileArchive::open_graph(ANDORRA_TILES).unwrap();

    let id = graph.tiles_in_bbox(ANDORRA_BBOX.0, ANDORRA_BBOX.1, GraphLevel::Local)[0];

    // No traffic mapping: every record reads back as "no reading".
    let without = valhalla::breaking::graph_tile_from_dir(&dir, id).unwrap();
    assert_eq!(
        without.live_traffic(&without.directededges()[0]).speed(),
        None
    );

    // Write 60 km/h for edge 0 through the traffic archive...
    let traffic_tile = traffic.traffic_tile(id).expect("traffic tile");
    traffic_tile.write_edge_traffic(0, valhalla::LiveTraffic::from_uniform_speed(60));

    // ...and read it back through a directory-backed graph tile.
    let with = valhalla::breaking::graph_tile_from_dir_with_traffic(&dir, id, &traffic).unwrap();
    assert_eq!(with.directededges().len(), without.directededges().len());
    assert_eq!(
        with.live_traffic(&with.directededges()[0]).speed(),
        Some(60)
    );
}

/// `LiveTraffic::UNKNOWN` is *a* value meaning "no reading", not *the* one: a tile built with no
/// traffic mapping reports `0x0FFFFFFF`, Valhalla's static invalid record. Both decode to
/// `speed() == None`, so behaviour is right, but `== LiveTraffic::UNKNOWN` is the wrong test and
/// today nothing says so.
#[test]
fn unknown_traffic_has_two_bit_patterns() {
    let dir = andorra_tile_dir().display().to_string();
    let graph = TileArchive::open_graph(ANDORRA_TILES).unwrap();
    let id = graph.tiles()[0];

    let tile = valhalla::breaking::graph_tile_from_dir(&dir, id).unwrap();
    let no_mapping = tile.live_traffic(&tile.directededges()[0]);

    assert_ne!(no_mapping, valhalla::LiveTraffic::UNKNOWN);
    assert_eq!(no_mapping.to_bits(), 0x0FFF_FFFF);
    assert_eq!(no_mapping.speed(), None);
    assert_eq!(valhalla::LiveTraffic::UNKNOWN.speed(), None);
}

// =============================================================================================
// The ports. The same four workloads written against each shape - a traversal that needs a
// cache, a traffic updater, a whole-tileset scan, and the start-node problem.
// =============================================================================================

/// A start node that has edges leaving its tile, so a traversal actually exercises the cache.
fn busy_start_node(graph: &TileArchive) -> GraphId {
    for tile_id in graph.tiles() {
        let tile = graph.graph_tile(tile_id).unwrap();
        for (i, node) in tile.nodes().iter().enumerate() {
            if tile.node_edges(node).iter().any(|de| de.leaves_tile()) {
                return GraphId::from_parts(
                    tile_id.level().repr as u32,
                    tile_id.tileid(),
                    i as u32,
                )
                .unwrap();
            }
        }
    }
    panic!("no node with a tile-leaving edge in the fixture");
}

mod reader {
    use super::*;
    use valhalla::breaking::{Cached, GraphReader, TileDir, TileExtract, TrafficExtract};

    fn reachable_nodes(source: &mut impl GraphReader, start: GraphId, hops: u32) -> usize {
        let mut frontier = vec![start];
        let mut seen = std::collections::HashSet::new();
        seen.insert(start);

        for _ in 0..hops {
            let mut next = Vec::new();
            for node_id in frontier.drain(..) {
                let Some(tile) = source.graph_tile(TileId::of(node_id)) else {
                    continue;
                };
                let Some(node) = tile.node(node_id.id()) else {
                    continue;
                };
                for de in tile.node_edges(node) {
                    let end = de.endnode();
                    if seen.insert(end) {
                        next.push(end);
                    }
                }
            }
            frontier = next;
        }
        seen.len()
    }

    /// Deep enough that the frontier leaves the start tile and comes back.
    const HOPS: u32 = 14;

    #[test]
    fn traversal_is_generic_over_every_backend() {
        let mut extract = TileExtract::open(ANDORRA_TILES).unwrap();
        let start = busy_start_node(&TileArchive::open_graph(ANDORRA_TILES).unwrap());
        let expected = reachable_nodes(&mut extract, start, HOPS);
        assert!(
            expected > 100,
            "traversal too small to be interesting: {expected}"
        );

        // Same function, directory backend.
        let mut dir = TileDir::new(andorra_tile_dir().display().to_string());
        assert_eq!(reachable_nodes(&mut dir, start, HOPS), expected);

        // Same function, cached.
        let mut cached = Cached::new(extract);
        assert_eq!(reachable_nodes(&mut cached, start, HOPS), expected);
        assert!(cached.cached_tiles() > 1, "traversal crossed tiles");
    }

    /// The traffic updater. It never constructs a graph source, because traffic is its own axis.
    #[test]
    fn traffic_updater_needs_no_graph_source() {
        let (_tmp, path) = writable_traffic_copy();
        let traffic = TrafficExtract::open_writable(&path).unwrap();

        let mut written = 0;
        for id in traffic.tiles() {
            let tile = traffic.tile(id).expect("traffic tile");
            for edge in 0..tile.edge_count() {
                tile.write_edge_traffic(edge, valhalla::LiveTraffic::from_uniform_speed(50));
                written += 1;
            }
            tile.write_last_update(1_700_000_000);
        }
        assert!(written > 1000, "wrote {written} records");

        // Reading it back needs a graph source, and composing one is one call.
        let mut extract = TileExtract::open(ANDORRA_TILES)
            .unwrap()
            .with_traffic(TrafficExtract::open(&path).unwrap());
        let id = extract.tiles()[0];
        let tile = extract.graph_tile(id).unwrap();
        assert_eq!(
            tile.live_traffic(&tile.directededges()[0]).speed(),
            Some(50)
        );
    }

    /// Traffic joined onto a directory backend - the C++ combination.
    #[test]
    fn directory_backend_with_traffic() {
        let (_tmp, path) = writable_traffic_copy();
        let traffic = TrafficExtract::open_writable(&path).unwrap();
        let graph = TileArchive::open_graph(ANDORRA_TILES).unwrap();
        let id = graph.tiles()[0];
        traffic
            .tile(id)
            .unwrap()
            .write_edge_traffic(0, valhalla::LiveTraffic::from_uniform_speed(70));

        let mut dir = TileDir::new(andorra_tile_dir().display().to_string()).with_traffic(traffic);
        let tile = dir.graph_tile(id).unwrap();
        assert_eq!(
            tile.live_traffic(&tile.directededges()[0]).speed(),
            Some(70)
        );
    }

    fn count_edges(reader: &mut impl GraphReader) -> usize {
        reader
            .tiles()
            .into_iter()
            .filter_map(|id| reader.graph_tile(id))
            .map(|tile| tile.directededges().len())
            .sum()
    }

    #[test]
    fn whole_tileset_scan_runs_over_every_backend() {
        let mut extract = TileExtract::open(ANDORRA_TILES).unwrap();
        assert_eq!(count_edges(&mut extract), 30418);
        assert_eq!(count_edges(&mut Cached::new(extract.clone())), 30418);

        let mut dir = TileDir::new(andorra_tile_dir().display().to_string());
        assert_eq!(count_edges(&mut dir), 30418);
    }

    /// Only tiles the reader holds, the same on every backend, and the same as the reader it
    /// replaces.
    #[test]
    fn tiles_in_bbox_lists_what_the_reader_holds() {
        let mut extract = TileExtract::open(ANDORRA_TILES).unwrap();
        let dir = TileDir::new(andorra_tile_dir().display().to_string());
        let config = valhalla::Config::from_tile_extract(ANDORRA_TILES).unwrap();
        let today = valhalla::GraphReader::new(&config).unwrap();
        let (min, max) = ANDORRA_BBOX;

        let sorted = |mut tiles: Vec<TileId>| {
            tiles.sort();
            tiles
        };
        for level in [GraphLevel::Highway, GraphLevel::Arterial, GraphLevel::Local] {
            let ours = sorted(extract.tiles_in_bbox(min, max, level));
            assert!(!ours.is_empty(), "level {}", level.repr);
            assert_eq!(sorted(dir.tiles_in_bbox(min, max, level)), ours);
            let theirs = today.tiles_in_bbox(min, max, level);
            assert_eq!(sorted(theirs.into_iter().map(TileId::of).collect()), ours);

            let covering = TileId::covering(min, max, level);
            assert!(
                ours.iter()
                    .all(|id| covering.contains(id) && extract.graph_tile(*id).is_some())
            );
        }
    }

    #[test]
    fn a_directory_lists_and_dates_the_same_tileset() {
        let extract = TileExtract::open(ANDORRA_TILES).unwrap();
        let dir = TileDir::new(andorra_tile_dir().display().to_string());

        let (mut ours, mut theirs) = (dir.tiles(), extract.tiles());
        ours.sort();
        theirs.sort();
        assert_eq!(ours, theirs);
        assert_eq!(dir.dataset_id(), extract.dataset_id());

        let missing = TileDir::new("/nonexistent");
        assert!(missing.tiles().is_empty());
        assert_eq!(missing.dataset_id(), 0);
    }

    /// The start-node problem, against a cache so the extra tile loads are cheap.
    #[test]
    fn edge_nodes_resolves_both_ends() {
        let mut cached = Cached::new(TileExtract::open(ANDORRA_TILES).unwrap());
        let tile_id = cached.tiles()[0];
        let tile = cached.graph_tile(tile_id).unwrap();

        let mut checked = 0;
        for (i, de) in tile.directededges().iter().enumerate().take(200) {
            let edge_id =
                GraphId::from_parts(tile_id.level().repr as u32, tile_id.tileid(), i as u32)
                    .unwrap();
            let Some((start, end)) = cached.edge_nodes(edge_id) else {
                continue;
            };
            assert_eq!(end, de.endnode());
            // The start node must list this edge among its own.
            let start_tile = cached.graph_tile(TileId::of(start)).unwrap();
            let start_node = start_tile.node(start.id()).unwrap();
            let edges = start_tile.node_edges(start_node);
            assert!(
                !edges.is_empty(),
                "start node {start:?} for edge {edge_id:?} has no edges"
            );
            checked += 1;
        }
        assert!(checked > 100, "only checked {checked} edges");
    }
}

/// `TrafficTile`'s write methods are safe fns that write straight into the mmap. On a read-only
/// mapping that is a SIGBUS, not a panic or an error - found by running shape C's updater against
/// a read-only archive.
///
/// It cannot happen today only because `GraphReader::new` hardcodes a writable traffic mapping.
/// Any shape that lets the caller choose read-only makes these four methods unsound, so the
/// read/write distinction has to be in the type, not a bool.
#[test]
#[ignore = "crashes the process with SIGBUS - run explicitly to confirm"]
fn writing_to_a_readonly_traffic_mapping_is_a_sigbus() {
    let (_tmp, path) = writable_traffic_copy();
    let traffic = TileArchive::open_traffic(&path).unwrap(); // read-only
    let id = traffic.tiles()[0];
    let tile = traffic.traffic_tile(id).unwrap();
    tile.write_edge_traffic(0, valhalla::LiveTraffic::from_uniform_speed(50)); // SIGBUS
}

// =============================================================================================
// What is shareable. Decided by `GraphTile: !Send`, not by the trait.
// =============================================================================================

/// Compile-time `!Send` probe. Must expand at a *concrete* type: inside a generic function the
/// inherent impl below is never selected, which silently makes every check vacuous.
macro_rules! assert_not_send {
    ($t:ty) => {{
        #[allow(dead_code)]
        trait NotSend {
            const IS_SEND: bool = false;
        }
        impl<T> NotSend for T {}
        struct Wrap<T>(std::marker::PhantomData<T>);
        #[allow(dead_code)]
        impl<T: Send> Wrap<T> {
            const IS_SEND: bool = true;
        }
        // Inherent const wins when `$t: Send`; otherwise resolution falls back to the blanket trait.
        !<Wrap<$t>>::IS_SEND
    }};
}

#[test]
fn thread_safety_matrix() {
    fn assert_send_sync<T: Send + Sync>() {}

    // The probe is not vacuous.
    assert!(
        !assert_not_send!(u32),
        "probe broken: u32 should read as Send"
    );
    assert!(
        assert_not_send!(std::rc::Rc<u32>),
        "probe broken: Rc should read as !Send"
    );

    // Sources are Send + Sync: they hold only the mmap index, and hand out tiles per call.
    assert_send_sync::<TileArchive>();
    assert_send_sync::<valhalla::breaking::TileExtract>();
    assert_send_sync::<valhalla::breaking::TileDir>();
    assert_send_sync::<valhalla::breaking::TrafficExtract>();

    // Tiles are neither, because of the non-atomic intrusive refcount.
    assert!(assert_not_send!(valhalla::GraphTile));

    // ...so no tile cache can be either. That is why `RefCell` costs nothing a
    // `Mutex` would have had to: a cache shared across threads was never expressible.
    assert!(assert_not_send!(
        valhalla::breaking::Cached<valhalla::breaking::TileExtract>
    ));

    // A traffic tile is not Send either - but only because of its raw pointers. Its accessors are
    // already volatile and coherence-safe by construction, so this one reads as an oversight
    // rather than a constraint, and it blocks a multi-threaded traffic updater.
    assert!(assert_not_send!(valhalla::TrafficTile));
}

/// Keeping `GraphReader` object-safe *and* accepting a `GraphId` at call sites: generic methods
/// go on a blanket-implemented extension trait, the object-safe method stays on the trait.
/// This is the `Iterator`/`Itertools`, `AsyncRead`/`AsyncReadExt` pattern.
mod tile_id_ergonomics {
    use super::*;
    use valhalla::GraphTile;
    use valhalla::breaking::{GraphReader, TileExtract};

    /// Convenience layer, blanket-implemented. Generic methods live here, not on the object-safe trait.
    trait GraphReaderExt: GraphReader {
        fn tile(&mut self, id: impl Into<TileId>) -> Option<GraphTile> {
            self.graph_tile(id.into())
        }
    }
    impl<R: GraphReader + ?Sized> GraphReaderExt for R {}

    #[test]
    fn ext_trait_keeps_object_safety_and_removes_call_site_noise() {
        let mut extract = TileExtract::open("tests/andorra/tiles.tar").unwrap();
        let archive = TileArchive::open_graph("tests/andorra/tiles.tar").unwrap();
        let tile_id = archive.tiles()[0];

        // A node id, as a traversal actually has.
        let node_id =
            valhalla::GraphId::from_parts(tile_id.level().repr as u32, tile_id.tileid(), 7)
                .unwrap();

        // Call sites read exactly like today - no `TileId::of(..)`.
        assert!(extract.tile(node_id).is_some());
        assert!(extract.tile(tile_id).is_some());

        // ...and the trait is still object-safe, so the factory still works.
        let mut boxed: Box<dyn GraphReader> = Box::new(extract);
        assert!(boxed.graph_tile(tile_id).is_some());
        assert!(boxed.tile(node_id).is_some());
        assert!(boxed.tile(tile_id).is_some());
    }
}

/// The gap, in one test: the same config routes but cannot read tiles.
#[test]
#[cfg(feature = "proto")]
fn actor_honours_tile_dir_but_graph_reader_does_not() {
    let config = valhalla::ConfigBuilder {
        mjolnir: valhalla::config::Mjolnir {
            tile_dir: andorra_tile_dir().display().to_string(),
            tile_extract: String::new(),
            traffic_extract: String::new(),
            ..Default::default()
        },
        ..Default::default()
    }
    .build();

    // Actor builds a real `baldr::GraphReader`, so it routes off the directory just fine.
    let mut actor = valhalla::Actor::new(&config).expect("actor from tile_dir");
    let request = valhalla::proto::Options {
        locations: vec![valhalla::proto::Location {
            ll: Some(valhalla::LatLon(42.5063, 1.5218).into()),
            ..Default::default()
        }],

        ..Default::default()
    };
    let located = actor.locate(&request);
    assert!(located.is_ok(), "locate failed: {:?}", located.err());

    // Our `GraphReader` is tile_extract only, so the same config is rejected.
    let reader = valhalla::GraphReader::new(&config);
    assert!(reader.is_err(), "expected tile_dir to be rejected today");
    println!("GraphReader::new said: {}", reader.err().unwrap());
}

/// How a `Send + Sync` source and a `!Send` cache coexist behind one trait.
///
/// The trait says nothing about threads. Auto traits belong to the concrete type, and a consumer
/// that shares states it with `+ Sync` at the use site. The cache is never shared: each thread
/// builds its own over a shared source - gitoxide's `Store` / `Handle` split.
mod thread_model {
    use super::*;
    use valhalla::breaking::{Cached, GraphReader, TileExtract};

    /// A parallel consumer asks for exactly what it needs: a source it can share.
    fn parallel_edge_count(source: &(impl GraphReader + Clone + Sync)) -> usize {
        let tiles = source.tiles();
        std::thread::scope(|s| {
            let workers: Vec<_> = tiles
                .chunks(2)
                .map(|chunk| {
                    s.spawn(move || {
                        // Per-thread cache over a clone of the shared reader. Built here, dropped here.
                        let mut cache = Cached::new(source.clone());
                        chunk
                            .iter()
                            .filter_map(|id| cache.graph_tile(*id))
                            .map(|tile| tile.directededges().len())
                            .sum::<usize>()
                    })
                })
                .collect();
            workers.into_iter().map(|w| w.join().unwrap()).sum()
        })
    }

    #[test]
    fn shared_source_per_thread_cache() {
        let extract = TileExtract::open(ANDORRA_TILES).unwrap();
        assert_eq!(parallel_edge_count(&extract), 30418);
    }

    /// Threads loading the same tile through their own clones of one reader get distinct
    /// `GraphTile` objects over the same read-only mapping: they share bytes, never a refcount.
    #[test]
    fn concurrent_loads_of_one_tile_share_bytes_not_refcounts() {
        let extract = TileExtract::open(ANDORRA_TILES).unwrap();
        let id = extract.tiles()[0];
        let edges: Vec<usize> = std::thread::scope(|s| {
            (0..8)
                .map(|_| {
                    let mut reader = extract.clone();
                    s.spawn(move || reader.graph_tile(id).unwrap().directededges().len())
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|w| w.join().unwrap())
                .collect()
        });
        assert!(edges.windows(2).all(|w| w[0] == w[1]));
    }
}

/// Tiles over a buffer whose ownership Rust hands to the tile - the S3-first direction. Rust
/// fetches the bytes; the tile then owns them, and nothing else does.
mod from_rust_memory {
    use super::*;
    use valhalla::breaking::graph_tile_from_memory;

    fn tile_bytes(id: TileId) -> Vec<u8> {
        std::fs::read(andorra_tile_dir().join(tile_file_suffix(id, false))).unwrap()
    }

    #[test]
    fn matches_the_extract() {
        let graph = TileArchive::open_graph(ANDORRA_TILES).unwrap();
        for id in graph.tiles() {
            let ours = graph_tile_from_memory(id, tile_bytes(id)).expect("tile from memory");
            let theirs = graph.graph_tile(id).unwrap();
            assert_eq!(ours.directededges().len(), theirs.directededges().len());
            assert_eq!(ours.nodes().len(), theirs.nodes().len());
            let (a, b) = (&ours.directededges()[0], &theirs.directededges()[0]);
            assert_eq!(ours.edgeinfo(a).way_id, theirs.edgeinfo(b).way_id);
        }
    }

    /// No copy, proved by address: the tile reads out of the very allocation that was moved in.
    #[test]
    fn tile_reads_straight_out_of_the_handed_over_buffer() {
        let id = TileArchive::open_graph(ANDORRA_TILES).unwrap().tiles()[0];
        let bytes = tile_bytes(id);
        let range = bytes.as_ptr_range();

        let tile = graph_tile_from_memory(id, bytes).unwrap();
        let de = &tile.directededges()[0];
        assert!(range.contains(&(tile.directededges().as_ptr() as *const u8)));
        assert!(range.contains(&(tile.nodes().as_ptr() as *const u8)));
        assert!(range.contains(&tile.edgeinfo(de).encoded_shape.as_ptr()));
    }

    /// The fetcher bug `GraphTile::Create` does not catch: valid bytes, wrong tile.
    #[test]
    fn bytes_of_another_tile_are_rejected() {
        let ids = TileArchive::open_graph(ANDORRA_TILES).unwrap().tiles();
        let (a, b) = (ids[0], ids[1]);
        assert!(graph_tile_from_memory(a, tile_bytes(a)).is_some());
        assert!(graph_tile_from_memory(b, tile_bytes(a)).is_none());
    }

    #[test]
    fn garbage_is_rejected() {
        let id = TileArchive::open_graph(ANDORRA_TILES).unwrap().tiles()[0];
        assert!(graph_tile_from_memory(id, vec![0u8; 4096]).is_none());
        assert!(graph_tile_from_memory(id, b"not a tile".to_vec()).is_none());
        assert!(graph_tile_from_memory(id, Vec::new()).is_none());
    }

    /// The channel model: a producer fetches bytes and sends them to workers. A `Vec<u8>` is
    /// `Send`, so it crosses the channel; the `GraphTile` built from it is `!Send`, so it stays on
    /// the worker that received the bytes. Ownership moves producer -> channel -> worker -> tile,
    /// and at no point does anyone else hold the buffer.
    #[test]
    fn producer_sends_bytes_workers_own_the_tiles() {
        let ids = TileArchive::open_graph(ANDORRA_TILES).unwrap().tiles();
        const WORKERS: usize = 3;

        let total: usize = std::thread::scope(|s| {
            let (senders, workers): (Vec<_>, Vec<_>) = (0..WORKERS)
                .map(|_| {
                    let (tx, rx) = std::sync::mpsc::channel::<(TileId, Vec<u8>)>();
                    let worker = s.spawn(move || {
                        rx.into_iter()
                            .filter_map(|(id, bytes)| graph_tile_from_memory(id, bytes))
                            .map(|tile| tile.directededges().len())
                            .sum::<usize>()
                    });
                    (tx, worker)
                })
                .unzip();

            // The producer - in production an async S3 range reader - just hands bytes over.
            for (i, id) in ids.iter().enumerate() {
                senders[i % WORKERS].send((*id, tile_bytes(*id))).unwrap();
            }
            drop(senders);
            workers.into_iter().map(|w| w.join().unwrap()).sum()
        });
        assert_eq!(total, 30418);
    }
}
