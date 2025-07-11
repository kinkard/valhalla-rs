use std::{
    hash::{Hash, Hasher},
    os::unix::ffi::OsStrExt,
    path::Path,
};

pub use ffi::DirectedEdge;
pub use ffi::EdgeInfo;
pub use ffi::EdgeUse;
pub use ffi::GraphId;
pub use ffi::GraphLevel;
pub use ffi::NodeInfo;
pub use ffi::TimeZoneInfo;
pub use ffi::TrafficEdge;

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

    // Edge use type. Indicates specialized uses.
    #[namespace = "valhalla::baldr"]
    #[cxx_name = "Use"]
    #[repr(u8)]
    enum EdgeUse {
        // Road specific uses
        kRoad = 0,
        kRamp = 1,            // Link - exits/entrance ramps.
        kTurnChannel = 2,     // Link - turn lane.
        kTrack = 3,           // Agricultural use, forest tracks
        kDriveway = 4,        // Driveway/private service
        kAlley = 5,           // Service road - limited route use
        kParkingAisle = 6,    // Access roads in parking areas
        kEmergencyAccess = 7, // Emergency vehicles only
        kDriveThru = 8,       // Commercial drive-thru (banks/fast-food)
        kCuldesac = 9,        // Cul-de-sac - dead-end road with possible circular end
        kLivingStreet = 10,   // Streets with preference towards bicyclists and pedestrians
        kServiceRoad = 11,    // Generic service road (not driveway, alley, parking aisle, etc.)

        // Bicycle specific uses
        kCycleway = 20,     // Dedicated bicycle path
        kMountainBike = 21, // Mountain bike trail

        kSidewalk = 24,

        // Pedestrian specific uses
        kFootway = 25,
        kSteps = 26, // Stairs
        kPath = 27,
        kPedestrian = 28,
        kBridleway = 29,
        kPedestrianCrossing = 32, // cross walks
        kElevator = 33,
        kEscalator = 34,
        kPlatform = 35,

        // Rest/Service Areas
        kRestArea = 30,
        kServiceArea = 31,

        // Other... currently, either BSS Connection or unspecified service road
        kOther = 40,

        // Ferry and rail ferry
        kFerry = 41,
        kRailFerry = 42,

        kConstruction = 43, // Road under construction

        // Transit specific uses. Must be last in the list
        kRail = 50,               // Rail line
        kBus = 51,                // Bus line
        kEgressConnection = 52,   // Connection between transit station and transit egress
        kPlatformConnection = 53, // Connection between transit station and transit platform
        kTransitConnection = 54,  // Connection between road network and transit egress
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

    /// Information held for each node within the graph. The graph uses a forward star structure:
    /// nodes point to the first outbound directed edge and each directed edge points to the other
    /// end node of the edge.
    struct NodeInfo {
        // With this definition and cxx's magic it becomes possible to do pointer arithmetic properly,
        // allowing to operate with slices of `NodeInfo` in Rust.
        // Otherwise, Rust compiler has no way to know the size of the `NodeInfo` struct and assumes that
        // `NodeInfo` is a zero-sized type (ZST), which leads to incorrect pointer arithmetic.
        // The whole Valhalla's ability to work with binary files (tilesets) relies this contract.
        data: [u64; 4],
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

    /// Helper struct to return a slice of nodes from C++ to Rust.
    struct NodeInfoSlice {
        /// Pointer to the first node in the span.
        ptr: *const NodeInfo,
        /// Number of nodes in the span.
        len: usize,
    }

    /// Information about the timezone, such as name and offset from UTC.
    #[derive(Clone)]
    struct TimeZoneInfo {
        /// Timezone name in the tz database.
        name: String,
        /// Offset in seconds from UTC for the timezone.
        offset_seconds: i32,
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
        /// Constructs a new `GraphId` from the given hierarchy level, tile ID, and unique ID within the tile.
        fn from_parts(level: u32, tileid: u32, id: u32) -> Result<GraphId>;

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
        fn nodes(tile: &GraphTile) -> NodeInfoSlice;
        fn node(self: &GraphTile, index: usize) -> Result<*const NodeInfo>;
        /// Retrieves all traffic flows for a given tile.
        /// todo: move it in Rust and implement via bindings.
        fn get_tile_traffic_flows(tile: &GraphTile) -> Vec<TrafficEdge>;

        #[namespace = "valhalla::baldr"]
        #[cxx_name = "Use"]
        type EdgeUse;

        #[namespace = "valhalla::baldr"]
        type DirectedEdge;
        /// End node of the directed edge.
        fn endnode(self: &DirectedEdge) -> GraphId;
        /// Returns the specialized use type of the edge.
        #[cxx_name = "use"]
        fn use_type(self: &DirectedEdge) -> EdgeUse;
        /// Returns the length of the edge in meters.
        fn length(self: &DirectedEdge) -> u32;
        /// Access modes in the forward direction. Bit mask using [`crate::access`] constants.
        #[rust_name = "forwardaccess_u32"]
        fn forwardaccess(self: &DirectedEdge) -> u32;
        /// Access modes in the reverse direction. Bit mask using [`crate::access`] constants.
        #[rust_name = "reverseaccess_u32"]
        fn reverseaccess(self: &DirectedEdge) -> u32;
        /// Returns the default speed in km/h for this edge.
        fn speed(self: &DirectedEdge) -> u32;
        /// Returns the free flow speed (typical speed during night, from 7pm to 7am) in km/h for this edge.
        fn free_flow_speed(self: &DirectedEdge) -> u32;
        /// Returns the constrained flow speed (typical speed during day, from 7am to 7pm) in km/h for this edge.
        fn constrained_flow_speed(self: &DirectedEdge) -> u32;
        /// Is this edge a shortcut edge.
        fn is_shortcut(self: &DirectedEdge) -> bool;

        #[namespace = "valhalla::baldr"]
        type NodeInfo;
        /// Access modes allowed to pass through the node. Bit mask using [`crate::access`] constants.
        fn access(self: &NodeInfo) -> u16;
        /// Time zone index of the node. Corresponding [`crate::TimeZoneInfo`] can be retrieved
        /// using [`crate::TimeZoneInfo::from_id`].
        fn timezone(self: &NodeInfo) -> u32;

        fn from_id(id: u32, unix_timestamp: u64) -> Result<TimeZoneInfo>;
    }
}

// Safety: All operations do not mutate [`TileSet`] inner state.
unsafe impl Send for ffi::TileSet {}
unsafe impl Sync for ffi::TileSet {}

// Safety: All operations do not mutate [`GraphTile`] inner state.
unsafe impl Send for ffi::GraphTile {}
unsafe impl Sync for ffi::GraphTile {}

/// Access bit field constants. Access in directed edge allows 12 bits.
pub mod access {
    pub const AUTO: u16 = 1;
    pub const PEDESTRIAN: u16 = 2;
    pub const BICYCLE: u16 = 4;
    pub const TRUCK: u16 = 8;
    pub const EMERGENCY: u16 = 16;
    pub const TAXI: u16 = 32;
    pub const BUS: u16 = 64;
    pub const HOV: u16 = 128;
    pub const WHEELCHAIR: u16 = 256;
    pub const MOPED: u16 = 512;
    pub const MOTORCYCLE: u16 = 1024;
    pub const ALL: u16 = 4095;
    pub const VEHICULAR: u16 = AUTO | TRUCK | MOPED | MOTORCYCLE | TAXI | BUS | HOV;
}

/// Coordinate in (lat, lon) format.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LatLon(pub f32, pub f32);

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

    /// Constructs a new `GraphId` from the given hierarchy level, tile ID, and unique ID within the tile.
    /// Returns `None` if the level is invalid (greater than 7) or if the tile ID is invalid (greater than 2^22).
    pub fn from_parts(level: u32, tileid: u32, id: u32) -> Option<Self> {
        ffi::from_parts(level, tileid, id).ok()
    }
}

/// High-level interface for reading Valhalla graph tiles from tar extracts.
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
///
/// `GraphTile` can outlive the [`GraphReader`] that created it. Cloning is cheap as it only
/// copies an atomic shared pointer (C++'s version of [`std::sync::Arc`]) to the underlying tile data.
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
        if slice.len == 0 {
            return &[]; // `std::slice::from_raw_parts` strictly requires a non-null pointer.
        }

        // Safety: correctness of the pointer arithmetic is checked by integration tests over a real dataset.
        // This works only because of the `data: [u64; 6]` definition in [`ffi::DirectedEdge`], as the Rust compiler
        // has no way of knowing the size of the `valhalla::baldr::DirectedEdge` struct and without that field Rust
        // assumes that `ffi::DirectedEdge` is a zero-sized type (ZST).
        // At the same time, Valhalla's entire ability to work with binary files (tilesets) relies on this contract.
        unsafe { std::slice::from_raw_parts(slice.ptr, slice.len) }
    }

    /// Index of the directed edge within the current tile if it exists.
    pub fn directededge(&self, index: u32) -> Option<&ffi::DirectedEdge> {
        match self.tile.directededge(index as usize) {
            Ok(ptr) if !ptr.is_null() => Some(unsafe { &*ptr }),
            // Valhalla always return non-null ptr if ok and throws an exception if the index is out of bounds.
            // But it also sounds nice to handle nullptr in the same way.
            _ => None,
        }
    }

    pub fn nodes(&self) -> &[ffi::NodeInfo] {
        let slice = ffi::nodes(&self.tile);
        if slice.len == 0 {
            return &[]; // `std::slice::from_raw_parts` strictly requires a non-null pointer.
        }

        // Safety: correctness of the pointer arithmetic is checked by integration tests over a real dataset.
        // This works only because of the `data: [u64; 4]` definition in [`ffi::NodeInfo`], as the Rust compiler
        // has no way of knowing the size of the `valhalla::baldr::NodeInfo` struct and without that field Rust
        // assumes that `ffi::NodeInfo` is a zero-sized type (ZST).
        // At the same time, Valhalla's entire ability to work with binary files (tilesets) relies on this contract.
        unsafe { std::slice::from_raw_parts(slice.ptr, slice.len) }
    }

    /// Index of the node within the current tile if it exists.
    pub fn node(&self, index: u32) -> Option<&ffi::NodeInfo> {
        match self.tile.node(index as usize) {
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

impl DirectedEdge {
    /// Access modes in the forward direction. Bit mask using [`crate::access`] constants.
    #[inline(always)]
    pub fn forwardaccess(&self) -> u16 {
        self.forwardaccess_u32() as u16
    }

    /// Access modes in the reverse direction. Bit mask using [`crate::access`] constants.
    #[inline(always)]
    pub fn reverseaccess(&self) -> u16 {
        self.reverseaccess_u32() as u16
    }
}

impl TimeZoneInfo {
    /// Retrieves the timezone information by its index if available. `unix_timestamp` is required to handle DST.
    pub fn from_id(id: u32, unix_timestamp: u64) -> Option<Self> {
        ffi::from_id(id, unix_timestamp).ok()
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
        assert_eq!(
            GraphId::from_parts(id.level(), id.tileid(), id.id()),
            Some(id)
        );

        let base = id.tile();
        assert_eq!(base.level(), 2);
        assert_eq!(base.tileid(), 838852);
        assert_eq!(base.id(), 0);
        assert_eq!(GraphId::from_parts(id.level(), id.tileid(), 0), Some(base));

        let default_id = GraphId::default();
        assert_eq!(default_id.level(), 7);

        assert_eq!(GraphId::from_parts(8, id.tileid(), 0), None);
    }
}
