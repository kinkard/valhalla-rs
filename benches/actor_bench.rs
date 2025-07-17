use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use valhalla::{Actor, Config, LatLon, proto};

const ANDORRA_CONFIG: &str = "tests/andorra/config.json";
const ANDORRA_TEST_LOC_1: LatLon = LatLon(42.50107335756198, 1.510341967860551); // Sant Julia de Loria
const ANDORRA_TEST_LOC_2: LatLon = LatLon(42.50627089323736, 1.521734167223563); // Andorra la Vella

fn route(c: &mut Criterion) {
    let config = Config::from_file(ANDORRA_CONFIG).unwrap();
    let mut actor = Actor::new(&config).unwrap();

    c.bench_function("route", |b| {
        let request = proto::Api {
            options: Some(proto::Options {
                costing_type: proto::costing::Type::Auto as i32,
                locations: vec![
                    proto::Location {
                        ll: Some(ANDORRA_TEST_LOC_1.into()),
                        ..Default::default()
                    },
                    proto::Location {
                        ll: Some(ANDORRA_TEST_LOC_2.into()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        };

        b.iter(|| {
            let response = actor.route(black_box(&request)).unwrap();
            black_box(response)
        });
    });
}

fn locate(c: &mut Criterion) {
    let config = Config::from_file(ANDORRA_CONFIG).unwrap();
    let mut actor = Actor::new(&config).unwrap();

    c.bench_function("locate", |b| {
        let request = proto::Api {
            options: Some(proto::Options {
                locations: vec![proto::Location {
                    ll: Some(ANDORRA_TEST_LOC_1.into()),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };

        b.iter(|| {
            let response = actor.locate(&request).unwrap();
            black_box(response)
        });
    });

    c.bench_function("locate json", |b| {
        let request = r#"{
            "locations":[{
                "lat":42.50107335756198,
                "lon":1.510341967860551
            }],
            "verbose": true
        }"#;
        b.iter(|| {
            let request =
                Actor::parse_api(black_box(request), proto::options::Action::Locate).unwrap();
            let response = actor.locate(&request).unwrap();
            black_box(response)
        });
    });
}

fn status(c: &mut Criterion) {
    let config = Config::from_file(ANDORRA_CONFIG).unwrap();
    let mut actor = Actor::new(&config).unwrap();

    c.bench_function("status", |b| {
        let request = proto::Api {
            options: Some(proto::Options {
                has_verbose: Some(proto::options::HasVerbose::Verbose(true)),
                ..Default::default()
            }),
            ..Default::default()
        };

        b.iter(|| {
            let response = actor.status(&request).unwrap();
            black_box(response)
        });
    });
}

criterion_group!(benches, route, locate, status);
criterion_main!(benches);
