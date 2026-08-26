//! Scratch bench for the 0.7 API evaluation. Measures what today's API costs.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use valhalla::{Config, GraphId, GraphReader};

const ANDORRA_TILES: &str = "tests/andorra/tiles.tar";

fn reader() -> GraphReader {
    GraphReader::new(&Config::from_tile_extract(ANDORRA_TILES).unwrap()).unwrap()
}

fn tiles(c: &mut Criterion) {
    let reader = reader();
    let tile_ids = reader.tiles();
    let edges: usize = tile_ids
        .iter()
        .map(|id| reader.graph_tile(*id).unwrap().directededges().len())
        .sum();
    println!("{} tiles, {edges} directed edges", tile_ids.len());

    let mut g = c.benchmark_group("tile");
    g.bench_function("graph_tile", |b| {
        b.iter(|| {
            for id in &tile_ids {
                black_box(reader.graph_tile(*id));
            }
        })
    });

    let cached: Vec<_> = tile_ids
        .iter()
        .map(|id| reader.graph_tile(*id).unwrap())
        .collect();
    g.bench_function("clone_cached_tile", |b| {
        b.iter(|| {
            for tile in &cached {
                black_box(tile.clone());
            }
        })
    });

    let tile = cached
        .iter()
        .max_by_key(|t| t.directededges().len())
        .unwrap();
    let count = tile.directededges().len() as u32;
    g.bench_function("directededge_by_index", |b| {
        b.iter(|| {
            for i in 0..count {
                black_box(tile.directededge(i));
            }
        })
    });
    g.bench_function("directededges_slice_index", |b| {
        b.iter(|| {
            for i in 0..count {
                black_box(tile.directededges().get(i as usize));
            }
        })
    });
    g.bench_function("hoisted_slice_index", |b| {
        b.iter(|| {
            let edges = tile.directededges();
            for i in 0..count {
                black_box(edges.get(i as usize));
            }
        })
    });
    g.bench_function("directededges_ffi_call", |b| {
        b.iter(|| black_box(tile.directededges().len()))
    });
    g.finish();
}

fn edge_info(c: &mut Criterion) {
    let reader = reader();
    let tiles: Vec<_> = reader
        .tiles()
        .into_iter()
        .map(|id| reader.graph_tile(id).unwrap())
        .collect();

    let mut g = c.benchmark_group("edgeinfo");
    g.bench_function("way_id_whole_tileset", |b| {
        b.iter(|| {
            let mut sum = 0u64;
            for tile in &tiles {
                for de in tile.directededges() {
                    sum += tile.edgeinfo(de).way_id();
                }
            }
            black_box(sum)
        })
    });
    g.bench_function("shape_points_whole_tileset", |b| {
        b.iter(|| {
            let mut sum = 0.0;
            for tile in &tiles {
                for de in tile.directededges() {
                    for point in tile.edgeinfo(de).shape() {
                        sum += point.0 + point.1;
                    }
                }
            }
            black_box(sum)
        })
    });
    g.bench_function("shape_len_whole_tileset", |b| {
        b.iter(|| {
            let mut sum = 0usize;
            for tile in &tiles {
                for de in tile.directededges() {
                    sum += tile.edgeinfo(de).shape().len();
                }
            }
            black_box(sum)
        })
    });
    // Andorra carries no elevation, so this measures the shim - pointer and slice, not the decode.
    g.bench_function("elevation_len_whole_tileset", |b| {
        b.iter(|| {
            let mut sum = 0usize;
            for tile in &tiles {
                for de in tile.directededges() {
                    sum += tile.edgeinfo(de).elevation(de, 0.0, 0.0).len();
                }
            }
            black_box(sum)
        })
    });
    g.finish();
}

fn graph_id(c: &mut Criterion) {
    /// What `GraphId::from_parts` does, without the FFI hop and the `Result`.
    fn from_parts_rust(level: u32, tileid: u32, id: u32) -> Option<GraphId> {
        if level > 7 || tileid > 0x3fffff || id > 0x1fffff {
            return None;
        }
        Some(GraphId::new(
            (level as u64) | ((tileid as u64) << 3) | ((id as u64) << 25),
        ))
    }

    let mut g = c.benchmark_group("graphid");
    g.bench_function("from_parts_ffi", |b| {
        b.iter(|| {
            let mut acc = 0u64;
            for i in 0..1000u32 {
                acc += GraphId::from_parts(black_box(2), black_box(838852), black_box(i))
                    .unwrap()
                    .value;
            }
            black_box(acc)
        })
    });
    g.bench_function("from_parts_rust", |b| {
        b.iter(|| {
            let mut acc = 0u64;
            for i in 0..1000u32 {
                acc += from_parts_rust(black_box(2), black_box(838852), black_box(i))
                    .unwrap()
                    .value;
            }
            black_box(acc)
        })
    });
    g.finish();
}

/// The pattern every hand-written traversal hits: resolve a tile for an id you just popped.
fn tile_lookup(c: &mut Criterion) {
    let reader = reader();
    let node_ids: Vec<GraphId> = reader
        .tiles()
        .into_iter()
        .flat_map(|tile_id| {
            let tile = reader.graph_tile(tile_id).unwrap();
            (0..tile.nodes().len() as u32)
                .map(move |i| GraphId::from_parts(tile_id.level(), tile_id.tileid(), i).unwrap())
        })
        .collect();
    println!("{} nodes", node_ids.len());

    let mut g = c.benchmark_group("traversal");
    g.bench_function("graph_tile_per_node", |b| {
        b.iter(|| {
            let mut sum = 0u32;
            for id in &node_ids {
                let tile = reader.graph_tile(*id).unwrap();
                sum += tile.node(id.id()).unwrap().edge_count();
            }
            black_box(sum)
        })
    });
    g.bench_function("cached_tile_per_node", |b| {
        b.iter(|| {
            let mut sum = 0u32;
            let mut cache: std::collections::HashMap<u64, valhalla::GraphTile> =
                std::collections::HashMap::new();
            for id in &node_ids {
                let tile = cache
                    .entry(id.tile().value)
                    .or_insert_with(|| reader.graph_tile(*id).unwrap());
                sum += tile.node(id.id()).unwrap().edge_count();
            }
            black_box(sum)
        })
    });
    g.finish();
}

/// `node_edges`/`edgeinfo`/`live_traffic` only `debug_assert!` that the reference belongs to the
/// tile, so in release a mixed-up tile reads out of bounds. What would checking always cost?
fn bounds_check(c: &mut Criterion) {
    let reader = reader();
    let tiles: Vec<_> = reader
        .tiles()
        .into_iter()
        .map(|id| reader.graph_tile(id).unwrap())
        .collect();

    let mut g = c.benchmark_group("bounds_check");
    g.bench_function("node_edges_unchecked", |b| {
        b.iter(|| {
            let mut sum = 0usize;
            for tile in &tiles {
                for node in tile.nodes() {
                    sum += tile.node_edges(node).len();
                }
            }
            black_box(sum)
        })
    });
    g.bench_function("node_edges_checked_rust_side", |b| {
        b.iter(|| {
            let mut sum = 0usize;
            for tile in &tiles {
                let nodes = tile.nodes();
                let range = nodes.as_ptr_range();
                for node in nodes {
                    let ptr = node as *const _;
                    assert!(range.contains(&ptr), "wrong tile");
                    sum += tile.node_edges(node).len();
                }
            }
            black_box(sum)
        })
    });
    g.finish();
}

criterion_group!(
    benches,
    tiles,
    edge_info,
    graph_id,
    tile_lookup,
    bounds_check
);
criterion_main!(benches);
