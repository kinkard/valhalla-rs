use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use valhalla::{Config, GraphId, GraphReader, LiveTraffic};

const ANDORRA_CONFIG: &str = "tests/andorra/config.json";

fn write_traffic(c: &mut Criterion) {
    let config = Config::from_file(ANDORRA_CONFIG).unwrap();
    let graph_reader = GraphReader::new(&config).unwrap();

    // find a tile the the most edges
    let mut max_edges = 0;
    let mut max_tile_id = GraphId::default();
    for tile_id in graph_reader.tiles() {
        let tile = graph_reader.graph_tile(tile_id).unwrap();

        let edge_count = tile.directededges().len();
        if edge_count > max_edges {
            max_edges = edge_count;
            max_tile_id = tile_id;
        }
    }

    let traffic_tile = graph_reader.traffic_tile(max_tile_id).unwrap();
    c.bench_function("write live traffic", |b| {
        b.iter(|| {
            for i in 0..max_edges {
                let traffic = LiveTraffic::from_uniform_speed((10 + i % 100) as u8);
                traffic_tile.write_edge_traffic(i as u32, black_box(traffic));
            }
        });
    });

    c.bench_function("read live traffic", |b| {
        b.iter(|| {
            for i in 0..max_edges {
                let traffic = traffic_tile.edge_traffic(i as u32);
                black_box(traffic);
            }
        });
    });

    c.bench_function("clear live traffic", |b| {
        b.iter(|| {
            traffic_tile.clear_traffic();
        });
    });
}

criterion_group!(benches, write_traffic);
criterion_main!(benches);
