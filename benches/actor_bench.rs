use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use valhalla::{Actor, Config, LatLon, proto};

const ANDORRA_CONFIG: &str = "tests/andorra/config.json";
const ANDORRA_TEST_LOC_1: LatLon = LatLon(42.50107335756198, 1.510341967860551); // Sant Julia de Loria
const ANDORRA_TEST_LOC_2: LatLon = LatLon(42.50627089323736, 1.521734167223563); // Andorra la Vella

fn route(c: &mut Criterion) {
    let config = Config::from_file(ANDORRA_CONFIG).unwrap();
    let mut actor = Actor::new(&config).unwrap();

    c.bench_function("short route", |b| {
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

    c.bench_function("long route", |b| {
        let request = proto::Api {
            options: Some(proto::Options {
                costing_type: proto::costing::Type::Auto as i32,
                locations: vec![
                    proto::Location {
                        ll: Some(LatLon(42.54381401912126, 1.4756460643803673).into()),
                        ..Default::default()
                    },
                    proto::Location {
                        ll: Some(LatLon(42.54262715333714, 1.7332292461658099).into()),
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

fn trace_attributes(c: &mut Criterion) {
    let config = Config::from_file(ANDORRA_CONFIG).unwrap();
    let mut actor = Actor::new(&config).unwrap();

    let shape = "ifadpAyon{A`ClBtAg@~FwKbOwIeAtL[lJt@hDnCtEdB~D|BnDbAj@v@Sr@s@?sAy@gIkA_ICyDh@{Cz@}At@k@fACbAb@j@~@xAvHvHn`@p@pE\\~@t@`@lAoA\\wCo@aRu@uSOoG`@mBr@oAbBw@dBNxA|Bj@|HbDj\\f@lBt@f@fAIr@uCtBaPbCaSj@eD~A{AjCk@nBrBb@jDyJr`AG~DnBzCbChAb@YbAo@bA}FzGmo@pAuHt@iC~AwBhAq@dBIbH~ApCXrBA`Ba@bBz@MvCeB`CeB`A}@f@yDjD}DlEsAlDE|AXrAr@j@|@OnAqAtBmFzAcC|C_@zEg@l@^P~@S|Bo@hA_B|@wBlAmAd@]fBTjCOlJqAbKx@rGFtCN~FcBjIqDxIwBlD?zBGzCg@Rc@^Yl@Ot@Cd@?f@Df@Jb@P^TZVTZLZD\\?ZGXOTSRYN]J_@Fc@|GwAbCG|DCnA@tDf@xCGvI_CfBu@~@i@ZQ|DyBtIeF|BuAdCoERkACoBUoBcBsDyBoCiB_C}@kBuAcEu@aDQw@]uCKsB@qAPkB^cC~@}EdBgGrUih@vAaDjEaJlMqYrMmY|@uA`AiAnDwDtDiDvBiCX[`BoBtDgFjB_ENg@Bk@x@uIb@wG`@eIJyAzAar@^qQ_AsUkCuRy@aGuE}PyDqLyFaJiDiFyMgRsImN_AuAeBsB}ByAkDy@mFe@oEYqF}AsHcDeB{@aA[g@M{Cy@gA[{Bm@{IyBkGaB{EgBmB{@aB}@mCoBgCoB}HaJmSmV}QmUqD{EeDgG}BsFoB{I}@sF_DaYu@yEs@mEs@qCw@eC_DuHeDaIaAaCwEgKkFgL}JkUwHoOqBuDqBuCcB}B_CyBuFeEmDoBwBqA_G_CsFyA_@IoEy@sEk@_DMuAKmBWy@a@w@y@Qq@E}FFW`@eB|@wAbA{@tAc@rBB`DvB~TpNhKtGlSvM|GhEjPlH`NpF";

    c.bench_function("trace attributes json", |b| {
        let request = proto::Api {
            options: Some(proto::Options {
                costing_type: proto::costing::Type::Auto as i32,
                has_encoded_polyline: Some(proto::options::HasEncodedPolyline::EncodedPolyline(
                    shape.into(),
                )),
                ..Default::default()
            }),
            ..Default::default()
        };
        b.iter(|| {
            let response = actor.trace_attributes(black_box(&request)).unwrap();
            black_box(response)
        });
    });

    c.bench_function("trace attributes pbf", |b| {
        let request = proto::Api {
            options: Some(proto::Options {
                costing_type: proto::costing::Type::Auto as i32,
                has_encoded_polyline: Some(proto::options::HasEncodedPolyline::EncodedPolyline(
                    shape.into(),
                )),
                format: proto::options::Format::Pbf as i32,
                ..Default::default()
            }),
            ..Default::default()
        };
        b.iter(|| {
            let response = actor.trace_attributes(black_box(&request)).unwrap();
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

criterion_group!(benches, route, trace_attributes, locate, status);
criterion_main!(benches);
