use std::{os::unix::ffi::OsStrExt, path::Path};

#[cxx::bridge]
mod ffi {
    /// Identifier of a node or an edge within the tiled, hierarchical graph.
    /// Includes the tile Id, hierarchy level, and a unique identifier within the tile/level.
    #[derive(Clone, Copy, Debug)]
    struct GraphId {
        value: u64,
    }

    /// Hierarchical graph level that defines the type of roads and their importance.
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
        include!("valhalla/src/libvalhalla.hpp");

        #[namespace = "valhalla::baldr"]
        type GraphId;
        /// Hierarchy level of the tile this identifier belongs to
        fn level(self: &GraphId) -> u32;
        /// Tile identifier of this GraphId within the hierarchy level
        fn tileid(self: &GraphId) -> u32;
        /// Combined tile information (level and tile id) as a single value
        #[cxx_name = "Tile_Base"]
        fn tile(self: &GraphId) -> GraphId;
        /// Identifier within the tile, unique within the tile and level
        fn id(self: &GraphId) -> u32;

        type GraphTile;

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
        ) -> Vec<GraphId>;
        fn get_tile(self: &TileSet, id: GraphId) -> SharedPtr<GraphTile>;

        /// Retrieves all traffic flows for a given tile.
        /// todo: move it in Rust and implement via bindings
        fn get_tile_traffic_flows(tile: &GraphTile) -> Vec<TrafficEdge>;
    }
}

// safery: All operations do not mutate [`TileSet`] inner state
unsafe impl Send for ffi::TileSet {}
unsafe impl Sync for ffi::TileSet {}

/// Coordinate in (lat, lon) format
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LatLon(pub f32, pub f32);

pub use ffi::GraphId;
pub use ffi::GraphLevel;
pub use ffi::TrafficEdge;

impl Default for GraphId {
    fn default() -> Self {
        Self {
            // `valhalla::baldr::kInvalidGraphId`
            value: 0x3fffffffffff,
        }
    }
}

impl PartialEq for GraphId {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

#[derive(Clone)]
pub struct GraphReader {
    tileset: cxx::SharedPtr<ffi::TileSet>,
}

impl GraphReader {
    pub fn new(config_file: &Path) -> Option<Self> {
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

    pub fn tiles_in_bbox(&self, min: LatLon, max: LatLon, level: GraphLevel) -> Vec<GraphId> {
        self.tileset
            .tiles_in_bbox(min.0, min.1, max.0, max.1, level)
    }

    pub fn get_tile_traffic_flows(&self, id: GraphId) -> Vec<TrafficEdge> {
        self.tileset
            .get_tile(id)
            .as_ref()
            .map(ffi::get_tile_traffic_flows)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_id() {
        let id = GraphId {
            value: 5411833275938,
        };
        assert_eq!(id.level(), 2);
        assert_eq!(id.tileid(), 838852);
        assert_eq!(id.id(), 161285);

        let base = id.tile();
        assert_eq!(base.level(), 2);
        assert_eq!(base.tileid(), 838852);
        assert_eq!(base.id(), 0);
    }
}
