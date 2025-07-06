use std::{
    hash::{Hash, Hasher},
    os::unix::ffi::OsStrExt,
    path::Path,
};

#[cxx::bridge]
mod ffi {
    /// Hierarchical graph level that defines the type of roads and their importance.
    #[derive(Clone, Copy, Debug)]
    enum GraphLevel {
        Highway = 0,
        Arterial = 1,
        Local = 2,
    }

    /// Identifier of a node or an edge within the tiled, hierarchical graph.
    /// Includes the tile Id, hierarchy level, and a unique identifier within the tile/level.
    #[derive(Clone, Copy, Debug, Eq)]
    struct GraphId {
        value: u64,
    }

    /// Directed edge within the graph.
    struct DirectedEdge {
        // With this definition and cxx's magic it becomes possible to do pointer arithmetic properly,
        // allowing to operate with slices of `DirectedEdge` in Rust.
        // Otherwise, Rust compiler has no way to know the size of the `DirectedEdge` struct and assumes that
        // `DirectedEdge` is a zero-sized type (ZST), which leads to incorrect pointer arithmetic.
        // The whole Valhalla's ability to work with binary files (tilesets) relies this contract.
        data: [u64; 6],
    }

    /// Dynamic (cold) information about the edge, such as OSM Way ID, speed limit, shape, elevation, etc.
    struct EdgeInfo {
        /// OSM Way ID of the edge.
        way_id: u64,
        /// Speed limit in km/h. 0 if not available and 255 if not limited (e.g. autobahn).
        speed_limit: u8,
        /// polyline6 encoded shape of the edge.
        shape: String,
    }

    /// Representation of the road graph edge with traffic information that contains a subset of data
    /// stored in [`valhalla::baldr::DirectedEdge`] and [`valhalla::baldr::EdgeInfo`] that is exposed to Rust.
    struct TrafficEdge {
        /// polyline6 encoded shape of the edge
        shape: String,
        /// Ratio between live speed and speed limit (or default edge speed if speed limit is unavailable).
        normalized_speed: f32,
    }

    /// Helper struct to return a slice of directed edges from C++ to Rust.
    struct DirectedEdgeSlice {
        /// Pointer to the first directed edge in the span.
        ptr: *const DirectedEdge,
        /// Number of directed edges in the span.
        len: usize,
    }

    unsafe extern "C++" {
        include!("valhalla/src/libvalhalla.hpp");

        type GraphLevel;

        #[namespace = "valhalla::baldr"]
        type GraphId;
        /// Hierarchy level of the tile this identifier belongs to.
        fn level(self: &GraphId) -> u32;
        /// Tile identifier of this GraphId within the hierarchy level.
        fn tileid(self: &GraphId) -> u32;
        /// Combined tile information (level and tile id) as a single value.
        #[cxx_name = "Tile_Base"]
        fn tile(self: &GraphId) -> GraphId;
        /// Identifier within the tile, unique within the tile and level.
        fn id(self: &GraphId) -> u32;

        type TileSet;
        fn new_tileset(config: &CxxString) -> Result<SharedPtr<TileSet>>;
        fn tiles(self: &TileSet) -> Vec<GraphId>;
        fn tiles_in_bbox(
            self: &TileSet,
            min_lat: f32,
            min_lon: f32,
            max_lat: f32,
            max_lon: f32,
            level: GraphLevel,
        ) -> Vec<GraphId>;
        fn get_tile(self: &TileSet, id: GraphId) -> SharedPtr<GraphTile>;

        type GraphTile;
        fn id(self: &GraphTile) -> GraphId;
        fn directededges(tile: &GraphTile) -> DirectedEdgeSlice;
        fn directededge(self: &GraphTile, index: usize) -> Result<*const DirectedEdge>;
        fn edgeinfo(tile: &GraphTile, de: &DirectedEdge) -> EdgeInfo;
        /// Retrieves all traffic flows for a given tile.
        /// todo: move it in Rust and implement via bindings.
        fn get_tile_traffic_flows(tile: &GraphTile) -> Vec<TrafficEdge>;

        #[namespace = "valhalla::baldr"]
        type DirectedEdge;
        /// Returns the length of the edge in meters.
        fn length(self: &DirectedEdge) -> u32;
        /// Returns the default speed in km/h for this edge.
        fn speed(self: &DirectedEdge) -> u32;
        /// Returns the free flow speed (typical speed during night, from 7pm to 7am) in km/h for this edge.
        fn free_flow_speed(self: &DirectedEdge) -> u32;
        /// Returns the constrained flow speed (typical speed during day, from 7am to 7pm) in km/h for this edge.
        fn constrained_flow_speed(self: &DirectedEdge) -> u32;
        /// Is this edge a shortcut edge.
        fn is_shortcut(self: &DirectedEdge) -> bool;
    }
}

// Safety: All operations do not mutate [`TileSet`] inner state.
unsafe impl Send for ffi::TileSet {}
unsafe impl Sync for ffi::TileSet {}

// Safety: All operations do not mutate [`GraphTile`] inner state.
unsafe impl Send for ffi::GraphTile {}
unsafe impl Sync for ffi::GraphTile {}

/// Coordinate in (lat, lon) format.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LatLon(pub f32, pub f32);

pub use ffi::DirectedEdge;
pub use ffi::EdgeInfo;
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

impl Hash for GraphId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl GraphId {
    pub fn new(value: u64) -> Self {
        Self { value }
    }
}

#[derive(Clone)]
pub struct GraphReader {
    tileset: cxx::SharedPtr<ffi::TileSet>,
}

impl GraphReader {
    /// Creates a new GraphReader from the given Valhalla configuration file.
    /// ```rust
    /// let reader = valhalla::GraphReader::from_file("path/to/config.json");
    /// ```
    pub fn from_file(config_file: impl AsRef<Path>) -> Option<Self> {
        cxx::let_cxx_string!(cxx_str = config_file.as_ref().as_os_str().as_bytes());
        let tileset = match ffi::new_tileset(&cxx_str) {
            Ok(tileset) => tileset,
            Err(err) => {
                println!("Failed to load tileset: {err:#}");
                return None;
            }
        };
        Some(Self { tileset })
    }

    /// Creates a new GraphReader from a Valhalla configuration JSON string.
    /// ```rust
    /// let config = r#"{"mjolnir":{"tile_extract":"path/to/tiles.tar","traffic_extract":"path/to/traffic.tar"}}"#;
    /// let reader = valhalla::GraphReader::from_json(&config);
    /// ```
    pub fn from_json(config_json: &str) -> Option<Self> {
        cxx::let_cxx_string!(cxx_str = config_json.as_bytes());
        let tileset = match ffi::new_tileset(&cxx_str) {
            Ok(tileset) => tileset,
            Err(err) => {
                println!("Failed to load tileset: {err:#}");
                return None;
            }
        };
        Some(Self { tileset })
    }

    /// Creates a new GraphReader from path to the tiles tar extract.
    /// ```rust
    /// let reader = valhalla::GraphReader::from_tile_extract("path/to/tiles.tar");
    /// ```
    pub fn from_tile_extract(tile_extract: impl AsRef<Path>) -> Option<Self> {
        let config = format!(
            "{{\"mjolnir\":{{\"tile_extract\":\"{}\"}}}}",
            tile_extract.as_ref().display()
        );
        Self::from_json(&config)
    }

    /// Graph tile object at given GraphId if it exists in the tileset.
    pub fn get_tile(&self, id: GraphId) -> Option<GraphTile> {
        GraphTile::new(self.tileset.get_tile(id))
    }

    /// List all tiles in the tileset.
    pub fn tiles(&self) -> Vec<GraphId> {
        self.tileset.tiles()
    }

    /// List all tiles in the bounding box for a given hierarchy level in the tileset.
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

/// Graph information for a tile within the Tiled Hierarchical Graph.
#[derive(Clone)]
pub struct GraphTile {
    tile: cxx::SharedPtr<ffi::GraphTile>,
}

impl GraphTile {
    fn new(tile: cxx::SharedPtr<ffi::GraphTile>) -> Option<Self> {
        if tile.is_null() {
            None
        } else {
            Some(Self { tile })
        }
    }

    /// GraphID of the tile, which includes the tile ID and hierarchy level.
    pub fn id(&self) -> GraphId {
        self.tile.id()
    }

    /// Slice of all directed edges in the current tile.
    pub fn directededges(&self) -> &[ffi::DirectedEdge] {
        let slice = ffi::directededges(&self.tile);
        // Safety: correctness of the pointer arithmetic is checked by integration tests over a real dataset.
        // This works only because of the `data: [u64; 6]` definition in [`ffi::DirectedEdge`], as Rust compiler
        // has no way to know the size of the `ffi::DirectedEdge` struct and without that field Rust assumes that
        // `ffi::DirectedEdge` is zero-sized type (ZST).
        // At the same time, whole Valhalla's ability to work with binary files (tilesets) relies this contract.
        unsafe { std::slice::from_raw_parts(slice.ptr, slice.len) }
    }

    /// Index of the directed edge within the current tile if it exists.
    pub fn directededge(&self, index: usize) -> Option<&ffi::DirectedEdge> {
        match self.tile.directededge(index) {
            Ok(ptr) if !ptr.is_null() => Some(unsafe { &*ptr }),
            // Valhalla always return non-null ptr if ok and throws an exception if the index is out of bounds.
            // But it also sounds nice to handle nullptr in the same way.
            _ => None,
        }
    }

    /// Dynamic (cold) information about the edge, such as OSM Way ID, speed limit, shape, elevation, etc.
    pub fn edgeinfo(&self, de: &ffi::DirectedEdge) -> ffi::EdgeInfo {
        ffi::edgeinfo(&self.tile, de)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_id() {
        let id = GraphId::new(5411833275938);
        assert_eq!(id.level(), 2);
        assert_eq!(id.tileid(), 838852);
        assert_eq!(id.id(), 161285);

        let base = id.tile();
        assert_eq!(base.level(), 2);
        assert_eq!(base.tileid(), 838852);
        assert_eq!(base.id(), 0);

        let default_id = GraphId::default();
        assert_eq!(default_id.level(), 7);
    }
}
