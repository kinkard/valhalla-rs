//! Map-matches a polyline, then asks the tiles what the response left out.
//!
//! Demonstrates:
//!   - `trace_attributes` with typed `proto::Options` in and `proto::Api` out
//!   - `trip_leg::Edge::id` is a `GraphId`, leading straight back into the graph
//!   - `encoded_polyline` is always polyline6
//!
//! Run from the `examples/` workspace:
//!   cargo run -p match-polyline -- \
//!     --tiles ../tests/andorra/tiles.tar \
//!     --traffic ../tests/andorra/traffic.tar \
//!     --polyline6 'qwnapA__c|A_CeOu@qEyAkMs@cISuFEePS_Ze@yG_A}EwNyc@iG_P_BoE'
//!
//! The bundled `traffic.tar` holds no readings, so the live column reads `no reading`.

use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};
use clap::Parser;
use valhalla::{
    Actor, ConfigBuilder, DirectedEdge, GraphId, GraphReader, LiveTraffic,
    proto::{
        self, RoadClass,
        options::{Format, HasEncodedPolyline::EncodedPolyline, HasSearchRadius::SearchRadius},
        trip_leg::Use,
    },
};

/// Valhalla's default of 50m is loose enough to snap onto parallel roads.
const SEARCH_RADIUS_M: f32 = 20.0;

#[derive(Parser)]
#[command(
    about = "Map-match a polyline, then cross-reference the matched edges against the tiles",
    group(clap::ArgGroup::new("input").required(true).args(["polyline5", "polyline6"]))
)]
struct Cli {
    /// Polyline with 5 decimal places of precision, as Google Maps encodes it.
    #[arg(long, group = "input")]
    polyline5: Option<String>,
    /// Polyline with 6 decimal places, which is what Valhalla speaks natively.
    #[arg(long, group = "input")]
    polyline6: Option<String>,
    /// Path to the Valhalla tiles.tar.
    #[arg(long)]
    tiles: PathBuf,
    /// Path to the live traffic.tar.
    #[arg(long)]
    traffic: PathBuf,
}

impl Cli {
    /// Valhalla's `encoded_polyline` is always polyline6, so precision-5 input is re-encoded first.
    fn polyline6(&self) -> String {
        match (&self.polyline5, &self.polyline6) {
            (Some(p5), None) => to_polyline6(p5),
            (None, Some(p6)) => p6.clone(),
            _ => unreachable!("clap enforces exactly one polyline flag"),
        }
    }
}

fn to_polyline6(polyline5: &str) -> String {
    polyline_iter::encode(6, polyline_iter::decode(5, polyline5))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let polyline6 = cli.polyline6();

    let config = ConfigBuilder {
        mjolnir: valhalla::config::Mjolnir {
            tile_extract: cli.tiles.display().to_string(),
            traffic_extract: cli.traffic.display().to_string(),
            ..Default::default()
        },
        // Valhalla logs to stdout by default, which would mix into this example's output.
        logging: valhalla::config::Logging {
            r#type: "std_err".to_string(),
            ..Default::default()
        },
        ..Default::default()
    }
    .build();

    let mut actor = Actor::new(&config).map_err(|e| anyhow!("failed to create Actor: {e}"))?;
    let reader =
        GraphReader::new(&config).map_err(|e| anyhow!("failed to open GraphReader: {e}"))?;

    let matched = trace(&mut actor, polyline6)?;
    if matched.is_empty() {
        bail!("nothing matched — is the polyline inside the tileset, and in the right precision?");
    }

    print_matches(&matched);
    println!();
    print_tile_details(&reader, &matched);
    Ok(())
}

/// One matched edge, as `trace_attributes` describes it.
struct Matched {
    id: GraphId,
    way_id: u64,
    name: String,
    use_type: Use,
    road_class: RoadClass,
    length_km: f32,
    source_along_edge: f32,
    target_along_edge: f32,
}

/// Map-matches the polyline, returning the edges it snapped to.
fn trace(actor: &mut Actor, polyline6: String) -> Result<Vec<Matched>> {
    let request = proto::Options {
        costing_type: proto::costing::Type::Auto as i32,
        format: Format::Pbf as i32,
        shape_match: proto::ShapeMatch::MapSnap as i32,
        has_encoded_polyline: Some(EncodedPolyline(polyline6)),
        has_search_radius: Some(SearchRadius(SEARCH_RADIUS_M)),
        ..Default::default()
    };

    let response = actor.trace_attributes(&request);
    let Ok(valhalla::Response::Pbf(api)) = response else {
        return Err(anyhow!("expected a protobuf response, got {response:?}"));
    };

    // `trip` and `edge` are optional, the rest are repeated fields — walk, do not index.
    let edges = api
        .trip
        .iter()
        .flat_map(|trip| &trip.routes)
        .flat_map(|route| &route.legs)
        .flat_map(|leg| &leg.node)
        .filter_map(|node| node.edge.as_ref());

    Ok(edges
        .map(|e| Matched {
            id: GraphId::new(e.id),
            way_id: e.way_id,
            // `name` is a repeated `StreetName`, not a string.
            name: e.name.first().map(|n| n.value.clone()).unwrap_or_default(),
            use_type: Use::try_from(e.r#use).unwrap_or(Use::KRoadUse),
            road_class: RoadClass::try_from(e.road_class).unwrap_or_default(),
            length_km: e.length_km,
            source_along_edge: e.source_along_edge,
            target_along_edge: e.target_along_edge,
        })
        .collect())
}

/// The matched edges as `trace_attributes` returned them.
fn print_matches(matched: &[Matched]) {
    let plural = if matched.len() == 1 { "edge" } else { "edges" };
    println!("matched {} {plural}", matched.len());
    println!(
        "{:<22} {:>11}  {:<24} {:<14} {:<12} {:>8}  along edge",
        "graph_id", "way_id", "name", "use", "road_class", "length"
    );
    for m in matched {
        println!(
            "{:<22} {:>11}  {:<24} {:<14} {:<12} {:>7.0}m  {:.2}..{:.2}",
            m.id.to_string(),
            m.way_id,
            truncate(&m.name, 24),
            format!("{:?}", m.use_type),
            format!("{:?}", m.road_class),
            m.length_km * 1000.0,
            m.source_along_edge,
            m.target_along_edge,
        );
    }
}

/// The graph around each edge: opposing edge, junction, density, live traffic.
fn print_tile_details(reader: &GraphReader, matched: &[Matched]) {
    println!("the same edges in the graph — none of this is in any routing response:");
    println!(
        "{:<22} {:<22} {:>8} {:>8}  live detail",
        "graph_id", "opposing edge", "junction", "density"
    );

    for m in matched {
        let Some(tile) = reader.graph_tile(m.id) else {
            continue;
        };
        let Some(de) = tile.directededge(m.id.id()) else {
            continue;
        };

        let (opposing_id, junction, density) = match junction_info(reader, de) {
            Some((id, edges, density)) => (id.to_string(), edges.to_string(), density.to_string()),
            None => ("-".into(), "-".into(), "-".into()),
        };

        let live = describe_live(tile.live_traffic(de));

        println!(
            "{:<22} {opposing_id:<22} {junction:>8} {density:>8}  {live}",
            m.id.to_string()
        );
    }
}

/// The edge running back the other way, via `opp_index` into the end node's edge list.
fn junction_info(reader: &GraphReader, de: &DirectedEdge) -> Option<(GraphId, u32, u32)> {
    let end_node_id = de.endnode();
    let end_tile = reader.graph_tile(end_node_id)?;
    let node = end_tile.node(end_node_id.id())?;
    let index = node.edge_index() + de.opp_index();
    let opposing = GraphId::from_parts(end_node_id.level(), end_node_id.tileid(), index)?;
    Some((opposing, node.edge_count(), node.density()))
}

/// Renders a live-traffic record, per sub-segment.
fn describe_live(traffic: LiveTraffic) -> String {
    let Some(overall) = traffic.speed() else {
        return "no reading".to_string();
    };
    if overall == 0 {
        return "closed".to_string();
    }

    let segments: Vec<String> = traffic
        .segments()
        .map(|s| {
            let speed = s.speed.map_or("?".to_string(), |kmh| format!("{kmh}"));
            let congestion = s
                .congestion
                .map_or(String::new(), |c| format!(" @{:.0}%", c * 100.0));
            format!(
                "{:.0}-{:.0}% {speed}{congestion}",
                s.range.0 * 100.0,
                s.range.1 * 100.0
            )
        })
        .collect();

    let incidents = if traffic.has_incidents() {
        " +incident"
    } else {
        ""
    };
    format!("{overall} km/h [{}]{incidents}", segments.join(" | "))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max - 1).chain(['…']).collect()
    }
}
