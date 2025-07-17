use valhalla::{
    Actor, Config, LatLon, Response,
    proto::{self, options::Format},
};

const ANDORRA_CONFIG: &str = "tests/andorra/config.json";
const ANDORRA_TEST_LOC_1: LatLon = LatLon(42.50107335756198, 1.510341967860551); // Sant Julia de Loria
const ANDORRA_TEST_LOC_2: LatLon = LatLon(42.50627089323736, 1.521734167223563); // Andorra la Vella

#[test]
fn smoke() {
    let config = Config::from_file(ANDORRA_CONFIG).unwrap();
    let actor = Actor::new(&config);
    assert!(actor.is_ok());
}

#[test]
fn request_response_format() {
    type CheckFn = fn(&anyhow::Result<Response>) -> Result<(), String>;
    let expect_json = |response: &anyhow::Result<Response>| match response {
        Ok(Response::Json(str)) if str.starts_with('{') && str.ends_with('}') => Ok(()),
        Ok(Response::Json(str)) if str.starts_with("[{") && str.ends_with("}]") => Ok(()),
        _ => Err(format!("Expected JSON response, got: {response:?}")),
    };
    let expect_pbf = |response: &anyhow::Result<Response>| match response {
        Ok(Response::Pbf(_)) => Ok(()),
        _ => Err(format!("Expected PBF response, got: {response:?}")),
    };
    let expect_other = |response: &anyhow::Result<Response>| match response {
        Ok(Response::Other(_)) => Ok(()),
        _ => Err(format!("Expected binary response, got: {response:?}")),
    };
    let expect_err = |response: &anyhow::Result<Response>| match response {
        Err(_) => Ok(()),
        _ => Err(format!("Expected error response, got: {response:?}")),
    };

    struct EndpointTest {
        name: &'static str,
        endpoint: fn(&mut Actor, &proto::Api) -> anyhow::Result<Response>,
        options: proto::Options,
        format_checks: Vec<(Format, CheckFn)>,
    }

    let base_options = proto::Options {
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
    };

    let tests = vec![
        EndpointTest {
            name: "route",
            endpoint: Actor::route,
            options: base_options.clone(),
            format_checks: vec![
                (Format::Json, expect_json),
                (Format::Osrm, expect_json),
                (Format::Pbf, expect_pbf),
                (Format::Gpx, expect_other),
                (Format::Geotiff, expect_err), // Geotiff requires GDAL
            ],
        },
        EndpointTest {
            name: "locate",
            endpoint: Actor::locate,
            options: proto::Options {
                locations: vec![proto::Location {
                    ll: Some(ANDORRA_TEST_LOC_1.into()),
                    ..Default::default()
                }],
                has_verbose: Some(proto::options::HasVerbose::Verbose(true)), // for more detailed output
                ..Default::default()
            },
            // `locate` always returns JSON
            format_checks: vec![
                (Format::Json, expect_json),
                (Format::Osrm, expect_json),
                (Format::Pbf, expect_json),
                (Format::Gpx, expect_json),
                (Format::Geotiff, expect_err), // todo: Valhalla handles it differently, to be fixed
            ],
        },
        EndpointTest {
            name: "matrix",
            endpoint: Actor::matrix,
            options: proto::Options {
                costing_type: proto::costing::Type::Auto as i32,
                sources: base_options.locations.clone(),
                targets: base_options.locations.clone(),
                ..Default::default()
            },
            format_checks: vec![
                (Format::Json, expect_json),
                (Format::Osrm, expect_json),
                (Format::Pbf, expect_pbf),
                // (Format::Gpx, expect_err), // todo: Valhalla causes `std::terminate` on this format
                (Format::Geotiff, expect_err),
            ],
        },
        EndpointTest {
            name: "optimized_route",
            endpoint: Actor::optimized_route,
            options: base_options.clone(),
            format_checks: vec![
                (Format::Json, expect_json),
                (Format::Osrm, expect_json),
                (Format::Pbf, expect_pbf),
                (Format::Gpx, expect_other),
                (Format::Geotiff, expect_err), // Geotiff requires GDAL
            ],
        },
        EndpointTest {
            name: "isochrone",
            endpoint: Actor::isochrone,
            options: proto::Options {
                costing_type: proto::costing::Type::Auto as i32,
                locations: vec![proto::Location {
                    ll: Some(ANDORRA_TEST_LOC_2.into()),
                    ..Default::default()
                }],
                contours: vec![proto::Contour {
                    has_time: Some(proto::contour::HasTime::Time(10.0)),
                    has_color: Some(proto::contour::HasColor::Color("ff0000".into())),
                    ..Default::default()
                }],
                ..Default::default()
            },
            format_checks: vec![
                (Format::Json, expect_json),
                // (Format::Osrm, expect_json), // todo: Valhalla causes `std::terminate` on this format
                (Format::Pbf, expect_pbf),
                // (Format::Gpx, expect_other), // todo: Valhalla causes `std::terminate` on this format
                (Format::Geotiff, expect_err), // Geotiff requires GDAL
            ],
        },
        EndpointTest {
            name: "trace_route",
            endpoint: Actor::trace_route,
            options: proto::Options {
                costing_type: proto::costing::Type::Auto as i32,
                has_encoded_polyline: Some(proto::options::HasEncodedPolyline::EncodedPolyline(
                    "qwnapA__c|A_CeOu@qEyAkMs@cISuFEePS_Ze@yG_A}EwNyc@iG_P_BoE".into(),
                )),
                ..Default::default()
            },
            format_checks: vec![
                (Format::Json, expect_json),
                (Format::Osrm, expect_json),
                (Format::Pbf, expect_pbf),
                (Format::Gpx, expect_other),
                (Format::Geotiff, expect_err), // Geotiff requires GDAL
            ],
        },
        EndpointTest {
            name: "trace_attributes",
            endpoint: Actor::trace_attributes,
            options: proto::Options {
                costing_type: proto::costing::Type::Auto as i32,
                has_encoded_polyline: Some(proto::options::HasEncodedPolyline::EncodedPolyline(
                    "qwnapA__c|A_CeOu@qEyAkMs@cISuFEePS_Ze@yG_A}EwNyc@iG_P_BoE".into(),
                )),
                ..Default::default()
            },
            format_checks: vec![
                (Format::Json, expect_json),
                (Format::Osrm, expect_json),
                (Format::Pbf, expect_pbf),
                (Format::Gpx, expect_other),
                (Format::Geotiff, expect_err), // Geotiff requires GDAL
            ],
        },
        EndpointTest {
            name: "transit_available",
            endpoint: Actor::transit_available,
            options: base_options.clone(),
            format_checks: vec![
                //  `transit_available` always returns JSON
                (Format::Json, expect_json),
                (Format::Osrm, expect_json),
                (Format::Pbf, expect_json),
                (Format::Gpx, expect_json),
                (Format::Geotiff, expect_err),
            ],
        },
        EndpointTest {
            name: "expansion",
            endpoint: Actor::expansion,
            options: proto::Options {
                action: proto::options::Action::Route as i32,
                has_expansion_action: Some(proto::options::HasExpansionAction::ExpansionAction(
                    proto::options::Action::Route as i32,
                )),
                ..base_options.clone()
            },
            format_checks: vec![
                (Format::Json, expect_json),
                (Format::Osrm, expect_json),
                (Format::Pbf, expect_json), // no PBF support for expansion yet
                (Format::Gpx, expect_other), // todo: It's actually json in Vec<u8>...
                (Format::Geotiff, expect_err),
            ],
        },
        EndpointTest {
            name: "centroid",
            endpoint: Actor::centroid,
            options: base_options.clone(),
            format_checks: vec![
                (Format::Json, expect_json),
                (Format::Osrm, expect_err), // no OSRM format support for centroid
                (Format::Pbf, expect_pbf),
                (Format::Gpx, expect_other), // todo: It's actually json in Vec<u8>...
                (Format::Geotiff, expect_err),
            ],
        },
        EndpointTest {
            name: "status",
            endpoint: Actor::status,
            options: proto::Options {
                has_verbose: Some(proto::options::HasVerbose::Verbose(true)),
                ..Default::default()
            },
            format_checks: vec![
                (Format::Json, expect_json),
                (Format::Osrm, expect_json),
                (Format::Pbf, expect_pbf),
                (Format::Gpx, expect_other), // todo: It's actually json in Vec<u8>...
                (Format::Geotiff, expect_err),
            ],
        },
    ];

    let config = Config::from_file(ANDORRA_CONFIG).unwrap();
    let mut actor = Actor::new(&config).unwrap();

    for test in tests {
        for (format, check) in test.format_checks {
            let request = proto::Api {
                options: Some(proto::Options {
                    format: format as i32,
                    ..test.options.clone()
                }),
                ..Default::default()
            };
            let response = (test.endpoint)(&mut actor, &request);
            assert_eq!(check(&response), Ok(()), "{:?} for {format:?}", test.name);
        }
    }
}

#[test]
fn parse_api() {
    assert!(Actor::parse_api("", proto::options::Action::Route).is_err());
    assert!(Actor::parse_api("{", proto::options::Action::Route).is_err());
    assert!(Actor::parse_api("}", proto::options::Action::Route).is_err());

    let json = r#"{"units":"kilometers","date_time":{"type":"current"},"costing":"auto","costing_options":{"auto":{"use_ferry":0.5,"use_rail_ferry":0.5,"use_highways":0.5,"use_tolls":0.5,"country_crossing_cost":0}},"alternates":2,"locations":[{"lat":42.50107335756198,"lon":1.510341967860551},{"lat":42.50627089323736,"lon":1.521734167223563}]}"#;
    let api = Actor::parse_api(json, proto::options::Action::Route).expect("Failed to parse API");
    let options = api.options.as_ref().expect("Options should be present");
    assert_eq!(options.units, proto::options::Units::Kilometers as i32);
    // assert_eq!(options.date_time.as_ref().unwrap().type_, proto::options::DateTimeType::Current as i32);
    assert_eq!(options.costing_type, proto::costing::Type::Auto as i32);
    assert_eq!(options.locations.len(), 2);
    assert_eq!(options.locations[0].ll, Some(ANDORRA_TEST_LOC_1.into()));
    assert_eq!(options.locations[1].ll, Some(ANDORRA_TEST_LOC_2.into()));
    assert_eq!(
        options.has_alternates,
        Some(proto::options::HasAlternates::Alternates(2))
    );

    let auto_costings = match options
        .costings
        .get(&(proto::costing::Type::Auto as i32))
        .and_then(|costing| costing.has_options.as_ref())
    {
        Some(proto::costing::HasOptions::Options(auto_costings)) => auto_costings,
        _ => panic!("Expected auto costing options to be present"),
    };
    assert_eq!(
        auto_costings.has_use_ferry,
        Some(proto::costing::options::HasUseFerry::UseFerry(0.5))
    );
    assert_eq!(
        auto_costings.has_use_rail_ferry,
        Some(proto::costing::options::HasUseRailFerry::UseRailFerry(0.5))
    );
    assert_eq!(
        auto_costings.has_use_highways,
        Some(proto::costing::options::HasUseHighways::UseHighways(0.5))
    );
    assert_eq!(
        auto_costings.has_use_tolls,
        Some(proto::costing::options::HasUseTolls::UseTolls(0.5))
    );
    assert_eq!(
        auto_costings.has_country_crossing_cost,
        Some(proto::costing::options::HasCountryCrossingCost::CountryCrossingCost(0.0))
    );
}
