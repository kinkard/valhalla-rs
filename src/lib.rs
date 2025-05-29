use std::{os::unix::ffi::OsStrExt, path::PathBuf};

pub use ffi::GraphLevel;

#[cxx::bridge]
mod ffi {
    enum GraphLevel {
        Highway,
        Arterial,
        Local,
    }

    /// Representation of the road graph edge with traffic information that contains a subset of
    /// data stored in [`valhalla::baldr::DirectedEdge`] and [`valhalla::baldr::EdgeInfo`] that
    /// is exposed to Rust.
    struct TrafficEdge {
        /// Polyline6 encoded shape of the edge
        shape: String,
        /// Ratio between live speed and speed limit (or default edge speed if speed limit is unavailable)
        normalized_speed: f32,
    }

    unsafe extern "C++" {
        include!("libvalhalla/src/libvalhalla.hpp");

        type GraphLevel;

        type TileSet;
        fn new_tileset(config_file: &CxxString) -> Result<SharedPtr<TileSet>>;
        fn tiles_in_bbox(
            self: &TileSet,
            min_lat: f32,
            min_lon: f32,
            max_lat: f32,
            max_lon: f32,
            level: GraphLevel,
        ) -> Vec<u64>;
        fn get_tile_traffic(self: &TileSet, id: u64) -> Vec<TrafficEdge>;
    }
}

// safery: All operations do not mutate [`TileSet`] inner state
unsafe impl Send for ffi::TileSet {}
unsafe impl Sync for ffi::TileSet {}

/// Coordinate in (lat, lon) format
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LatLon(pub f32, pub f32);

/// Road graph tile id
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileId(pub u64);

pub use ffi::TrafficEdge;

#[derive(Clone)]
pub struct GraphReader {
    tileset: cxx::SharedPtr<ffi::TileSet>,
}

impl GraphReader {
    pub fn new(config_file: PathBuf) -> Option<Self> {
        cxx::let_cxx_string!(cxx_str = config_file.as_os_str().as_bytes());
        let tileset = match ffi::new_tileset(&cxx_str) {
            Ok(tileset) => tileset,
            Err(err) => {
                println!("Failed to load tileset: {err:#}");
                return None;
            }
        };

        Some(Self { tileset })
    }

    pub fn tiles_in_bbox(&self, min: LatLon, max: LatLon, level: GraphLevel) -> Vec<TileId> {
        self.tileset
            .tiles_in_bbox(min.0, min.1, max.0, max.1, level)
            .into_iter()
            .map(TileId)
            .collect()
    }

    pub fn get_tile_traffic_flows(&self, id: TileId) -> Vec<TrafficEdge> {
        self.tileset.get_tile_traffic(id.0)
    }
}
