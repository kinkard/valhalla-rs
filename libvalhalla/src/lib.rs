use std::{os::unix::ffi::OsStrExt, path::PathBuf};

pub use ffi::GraphLevel;

#[cxx::bridge]
mod ffi {
    enum GraphLevel {
        Highway,
        Arterial,
        Local,
    }

    unsafe extern "C++" {
        include!("libvalhalla/src/libvalhalla.hpp");

        type TrafficEdge;
        fn shape(self: &TrafficEdge) -> &CxxString;
        fn jam_factor(self: &TrafficEdge) -> f32;

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
        fn get_tile_traffic(self: &TileSet, id: u64) -> UniquePtr<CxxVector<TrafficEdge>>;
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

/// Representation of the road graph edge with traffic information
pub struct TrafficEdge {
    /// Polyline6 encoded shape of the flow.
    pub shape: String,
    /// Ration between live speed and speed limit (or default edge speed if speed limit is unavailable).
    pub jam_factor: f32,
}

#[derive(Clone)]
pub struct GraphReader {
    tileset: cxx::SharedPtr<ffi::TileSet>,
}

impl GraphReader {
    pub fn new(config_file: PathBuf) -> Self {
        cxx::let_cxx_string!(cxx_str = config_file.as_os_str().as_bytes());
        Self {
            tileset: ffi::new_tileset(&cxx_str).unwrap(),
        }
    }

    pub fn tiles_in_bbox(&self, min: LatLon, max: LatLon, level: GraphLevel) -> Vec<TileId> {
        self.tileset
            .tiles_in_bbox(min.0, min.1, max.0, max.1, level)
            .into_iter()
            .map(TileId)
            .collect()
    }

    pub fn get_tile_traffic_flows(&self, id: TileId) -> Vec<TrafficEdge> {
        self.tileset
            .get_tile_traffic(id.0)
            .into_iter()
            .map(|flow| TrafficEdge {
                shape: flow.shape().to_string(),
                jam_factor: flow.jam_factor(),
            })
            .collect()
    }
}
