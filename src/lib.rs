use std::{
    fmt,
    hash::{Hash, Hasher},
};

use bitflags::bitflags;
use prost::Message;

mod actor;
mod config;
pub mod proto;

pub use actor::Actor;
pub use actor::Response;
pub use config::Config;
pub use ffi::AdminInfo;
pub use ffi::DirectedEdge;
pub use ffi::EdgeInfo;
pub use ffi::EdgeUse;
pub use ffi::GraphId;
pub use ffi::GraphLevel;
pub use ffi::NodeInfo;
pub use ffi::NodeTransition;
pub use ffi::RoadClass;
pub use ffi::TimeZoneInfo;
pub use ffi::TrafficTile;
pub use ffi::decode_weekly_speeds;
pub use ffi::encode_weekly_speeds;

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
    #[derive(Clone, Copy, Eq)]
    struct GraphId {
        value: u64,
    }

    /// Edge use type. Indicates specialized uses.
    #[namespace = "valhalla::baldr"]
    #[cxx_name = "Use"]
    #[repr(u8)]
    #[derive(Debug)]
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

    /// [Road class] or importance of an edge.
    ///
    /// [Road class]: https://wiki.openstreetmap.org/wiki/Key:highway#Roads
    #[namespace = "valhalla::baldr"]
    #[repr(u8)]
    #[derive(Debug, PartialOrd, Ord)]
    enum RoadClass {
        kMotorway = 0,
        kTrunk = 1,
        kPrimary = 2,
        kSecondary = 3,
        kTertiary = 4,
        kUnclassified = 5,
        kResidential = 6,
        kServiceOther = 7,
        /// [`DirectedEdge`] has only 3 bits for road class.
        kInvalid = 8,
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

    /// Records a transition between a node on the current tile and a node
    /// at the same position on a different hierarchy level. Stores the GraphId
    /// of the end node as well as a flag indicating whether the transition is
    /// upwards (true) or downwards (false).
    struct NodeTransition {
        data: [u64; 1],
    }

    /// Helper struct to pass coordinates in (lat, lon) format between C++ and Rust.
    struct LatLon {
        lat: f64,
        lon: f64,
    }

    /// Information about the administrative area, such as country or state.
    #[derive(Clone)]
    struct AdminInfo {
        /// Text name of the country or "None" if not available.
        country_text: String,
        /// Text name of the state or "None" if not available. May be empty if country has no states.
        state_text: String,
        /// ISO 3166-1 alpha-2 country code.
        country_iso: String,
        /// ISO 3166-2 subdivision code (state/province part only), e.g. 'CA' for 'US-CA'.
        state_iso: String,
    }

    /// Information about the timezone, such as name and offset from UTC.
    #[derive(Clone)]
    struct TimeZoneInfo {
        /// Timezone name in the tz database.
        name: String,
        /// Offset in seconds from UTC for the timezone.
        offset_seconds: i32,
    }

    /// An interface for writing live traffic information for the corresponding graph tile.
    ///
    /// Can be obtained via [`crate::GraphReader::traffic_tile()`].
    /// `TrafficTile` can outlive the [`GraphReader`] that created it.
    struct TrafficTile {
        /// Pointer to `valhalla::baldr::TrafficTileHeader` of the tile.
        header: *const u64,
        /// Pointer to the start of the array of `valhalla::baldr::TrafficSpeed` records for the tile.
        speeds: *const u64,
        /// Shared ownership of the underlying memory-mapped file.
        traffic_tar: SharedPtr<tar>,
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

        #[namespace = "boost::property_tree"]
        type ptree = crate::config::ffi::ptree;

        type TileSet;
        fn new_tileset(config: &ptree) -> Result<SharedPtr<TileSet>>;
        fn tiles(self: &TileSet) -> Vec<GraphId>;
        fn tiles_in_bbox(
            self: &TileSet,
            min_lat: f32,
            min_lon: f32,
            max_lat: f32,
            max_lon: f32,
            level: GraphLevel,
        ) -> Vec<GraphId>;
        fn get_graph_tile(self: &TileSet, id: GraphId) -> SharedPtr<GraphTile>;
        fn get_traffic_tile(self: &TileSet, id: GraphId) -> Result<TrafficTile>;
        fn dataset_id(self: &TileSet) -> u64;

        type GraphTile;
        fn id(self: &GraphTile) -> GraphId;
        // Returned slice works only because of the `data: [u64; 6]` definition in [`ffi::DirectedEdge`].
        fn directededges(tile: &GraphTile) -> &[DirectedEdge];
        fn directededge(self: &GraphTile, index: usize) -> Result<*const DirectedEdge>;
        fn edgeinfo(tile: &GraphTile, de: &DirectedEdge) -> EdgeInfo;
        // Returned slice works only because of the `data: [u64; 4]` definition in [`ffi::NodeInfo`].
        fn nodes(tile: &GraphTile) -> &[NodeInfo];
        fn node(self: &GraphTile, index: usize) -> Result<*const NodeInfo>;
        // Returned slice works only because of the `data: [u64; 1]` definition in [`ffi::NodeTransition`].
        fn transitions(tile: &GraphTile) -> &[NodeTransition];
        fn transition(self: &GraphTile, index: u32) -> Result<*const NodeTransition>;
        fn node_edges<'a>(tile: &'a GraphTile, node: &NodeInfo) -> &'a [DirectedEdge];
        fn node_transitions<'a>(tile: &'a GraphTile, node: &NodeInfo) -> &'a [NodeTransition];
        fn node_latlon(tile: &GraphTile, node: &NodeInfo) -> LatLon;
        fn admininfo(tile: &GraphTile, index: u32) -> Result<AdminInfo>;
        unsafe fn IsClosed(self: &GraphTile, de: *const DirectedEdge) -> bool;
        unsafe fn GetSpeed(
            self: &GraphTile,
            de: *const DirectedEdge,
            flow_mask: u8,
            seconds: u64,
            is_truck: bool,
            flow_sources: *mut u8,
            seconds_from_now: u64,
        ) -> u32;
        // Helper method that returns 0 if the edge is closed, 255 if live speed in unknown and speed in km/h otherwise.
        fn live_speed(tile: &GraphTile, de: &DirectedEdge) -> u8;

        #[namespace = "valhalla::midgard"]
        type tar;

        type TrafficTile;
        /// GraphID of the tile, which includes the tile ID and hierarchy level.
        fn id(self: &TrafficTile) -> GraphId;
        /// Seconds since epoch of the last update.
        fn last_update(self: &TrafficTile) -> u64;
        /// Custom spare value stored in the header.
        fn spare(self: &TrafficTile) -> u64;
        /// Number of directed edges in this traffic tile.
        fn edge_count(self: &TrafficTile) -> u32;
        /// Live traffic information for the given edge index in the tile.
        fn edge_traffic(tile: &TrafficTile, edge_index: u32) -> Result<u64>;
        /// Writes the last update timestamp to the memory-mapped file.
        fn write_last_update(self: &TrafficTile, unix_timestamp: u64);
        /// Writes a custom value to the spare field in the memory-mapped file.
        fn write_spare(self: &TrafficTile, spare: u64);
        /// Writes live traffic information for the given edge index in the tile.
        fn write_edge_traffic(tile: &TrafficTile, edge_index: u32, traffic: u64) -> Result<()>;
        /// Clears live traffic information in the tile and sets the last update time to 0.
        /// The spare field is left unchanged.
        fn clear_traffic(self: &TrafficTile);

        #[namespace = "valhalla::baldr"]
        #[cxx_name = "Use"]
        type EdgeUse;

        #[namespace = "valhalla::baldr"]
        type RoadClass;

        #[namespace = "valhalla::baldr"]
        type DirectedEdge;
        /// End node of the directed edge. [`DirectedEdge::leaves_tile()`] returns true if end node is in a different tile.
        ///
        /// # Examples
        ///
        /// ```
        /// # fn example(reader: &valhalla::GraphReader, tile: &valhalla::GraphTile, edge: &valhalla::DirectedEdge) -> Option<()> {
        /// let end_node_id = edge.endnode();
        /// // Alternatively, check that `end_node_id.tile()` is different from `tile.id()`.
        /// let end_tile = if edge.leaves_tile() {
        ///     reader.graph_tile(end_node_id)?
        /// } else {
        ///     tile.clone()  // `clone()`
        /// };
        /// let end_node = end_tile.node(end_node_id.id())?;
        /// # Some(())
        /// # }
        /// ```
        fn endnode(self: &DirectedEdge) -> GraphId;
        /// The index of the opposing directed edge at the end node of this directed edge.
        ///
        /// # Examples
        ///
        /// ```
        /// # fn example(reader: &valhalla::GraphReader, tile: &valhalla::GraphTile, edge: &valhalla::DirectedEdge) -> Option<()> {
        /// let end_node_id = edge.endnode();
        /// // Alternatively, check that `end_node_id.tile()` is different from `tile.id()`.
        /// let end_tile = if edge.leaves_tile() {
        ///     reader.graph_tile(end_node_id)?
        /// } else {
        ///     tile.clone()  // `clone()`
        /// };
        /// let end_node = end_tile.node(end_node_id.id())?;
        /// let opp_edge = &end_tile.node_edges(end_node)[edge.opp_index() as usize];
        /// # Some(())
        /// # }
        /// ```
        fn opp_index(self: &DirectedEdge) -> u32;
        /// Specialized use type of the edge.
        #[cxx_name = "use"]
        fn use_type(self: &DirectedEdge) -> EdgeUse;
        /// Road class or importance of the edge.
        #[cxx_name = "classification"]
        fn road_class(self: &DirectedEdge) -> RoadClass;
        /// Length of the edge in meters.
        fn length(self: &DirectedEdge) -> u32;
        /// Whether this edge is part of a toll road.
        fn toll(self: &DirectedEdge) -> bool;
        /// Whether this edge is private or destination-only access.
        fn destonly(self: &DirectedEdge) -> bool;
        /// Whether this edge is part of a tunnel.
        fn tunnel(self: &DirectedEdge) -> bool;
        /// Whether this edge is part of a bridge.
        fn bridge(self: &DirectedEdge) -> bool;
        /// Whether this edge is part of a roundabout.
        fn roundabout(self: &DirectedEdge) -> bool;
        /// Whether this edge crosses a country border.
        #[cxx_name = "ctry_crossing"]
        fn crosses_country_border(self: &DirectedEdge) -> bool;
        /// Access modes in the forward direction. Bit mask using [`crate::Access`] constants.
        #[cxx_name = "forwardaccess"]
        fn forwardaccess_u32(self: &DirectedEdge) -> u32;
        /// Access modes in the reverse direction. Bit mask using [`crate::Access`] constants.
        #[cxx_name = "reverseaccess"]
        fn reverseaccess_u32(self: &DirectedEdge) -> u32;
        /// Default speed in km/h for this edge.
        fn speed(self: &DirectedEdge) -> u32;
        /// Truck speed in km/h for this edge.
        fn truck_speed(self: &DirectedEdge) -> u32;
        /// Free flow speed (typical speed during night, from 7pm to 7am) in km/h for this edge.
        fn free_flow_speed(self: &DirectedEdge) -> u32;
        /// Constrained flow speed (typical speed during day, from 7am to 7pm) in km/h for this edge.
        fn constrained_flow_speed(self: &DirectedEdge) -> u32;
        /// Whether this edge is a shortcut edge.
        fn is_shortcut(self: &DirectedEdge) -> bool;
        /// Whether this directed edge ends in a different tile.
        fn leaves_tile(self: &DirectedEdge) -> bool;

        #[namespace = "valhalla::baldr"]
        type NodeInfo;
        /// Get the index of the first outbound edge from this node. Since all outbound edges are
        /// in the same tile/level as the node we only need an index within the tile.
        fn edge_index(self: &NodeInfo) -> u32;
        /// Get the number of outbound directed edges from this node on the current hierarchy level.
        fn edge_count(self: &NodeInfo) -> u32;
        /// Elevation of the node in meters. Returns `-500.0` if elevation data is not available.
        fn elevation(self: &NodeInfo) -> f32;
        /// Access modes allowed to pass through the node. Bit mask using [`crate::Access`] constants.
        #[cxx_name = "access"]
        fn access_u16(self: &NodeInfo) -> u16;
        /// Index of the administrative area (country) the node is in. Corresponding [`crate::AdminInfo`] can be
        /// retrieved using [`crate::GraphTile::admin_info()`].
        fn admin_index(self: &NodeInfo) -> u32;
        /// Time zone index of the node. Corresponding [`crate::TimeZoneInfo`] can be retrieved
        /// using [`crate::TimeZoneInfo::from_id()`].
        fn timezone(self: &NodeInfo) -> u32;
        /// Relative road density in the area surrounding the node [0,15]. Higher values indicate more roads nearby.
        /// 15: Avenue des Champs-Elysees in Paris.
        /// 10: Lombard Street in San Francisco.
        /// 9: Unter den Linden in Berlin.
        /// 3: Golden Gate Bridge in San Francisco at the southern end.
        /// 0: Any rural area.
        fn density(self: &NodeInfo) -> u32;
        /// Get the index of the first transition from this node.
        fn transition_index(self: &NodeInfo) -> u32;
        /// Get the number of transitions from this node.
        fn transition_count(self: &NodeInfo) -> u32;

        /// Retrieves the timezone information by its index. `unix_timestamp` is required to handle DST/SDT.
        fn from_id(id: u32, unix_timestamp: u64) -> Result<TimeZoneInfo>;

        #[namespace = "valhalla::baldr"]
        type NodeTransition;
        /// Graph id of the corresponding node on another hierarchy level.
        fn endnode(self: &NodeTransition) -> GraphId;
        /// Is the transition up to a higher level.
        #[cxx_name = "up"]
        fn upward(self: &NodeTransition) -> bool;

        /// Encodes weekly speed data into a DCT-II compressed base64 string for Valhalla [historical traffic].
        ///
        /// Takes 2016 speed values (one per 5-minute interval covering a full week starting from
        /// Sunday 00:00) and returns a base64-encoded DCT-II compressed representation suitable for
        /// the `valhalla_add_predicted_traffic` tool's CSV input.
        /// N.B.: The encoding is lossy (2016 -> 200 coefficients). Use [`decode_weekly_speeds`] to
        /// evaluate compression quality if needed.
        ///
        /// # Examples
        /// ```
        /// // Generate sample weekly speed profile (constant 50 km/h)
        /// let speeds = vec![50.0; 2016];
        /// let encoded = valhalla::encode_weekly_speeds(&speeds).expect("Failed to encode");
        /// // Use in CSV: "1/47701/130,50,40,{encoded}"
        /// ```
        ///
        /// [historical traffic]: https://valhalla.github.io/valhalla/mjolnir/historical_traffic/#historical-traffic
        fn encode_weekly_speeds(speeds: &[f32]) -> Result<String>;

        /// Decodes a DCT-II compressed base64 string back to 2016 weekly speed values.
        ///
        /// Reconstructs the original weekly speed profile from its compressed representation using
        /// DCT-III inverse transform. Returns 2016 speed values (one per 5-minute interval covering
        /// a full week starting from Sunday 00:00). Useful for validating encoding quality since
        /// the compression is lossy (2016 -> 200 -> 2016 coefficients).
        ///
        /// [historical traffic]: https://valhalla.github.io/valhalla/mjolnir/historical_traffic/#historical-traffic
        fn decode_weekly_speeds(encoded: &str) -> Result<Vec<f32>>;
    }

    unsafe extern "C++" {
        include!("valhalla/src/costing.hpp");

        #[namespace = "valhalla::sif"]
        type DynamicCost;
        #[cxx_name = "Allowed"]
        unsafe fn NodeAllowed(self: &DynamicCost, node: *const NodeInfo) -> bool;
        unsafe fn IsAccessible(self: &DynamicCost, edge: *const DirectedEdge) -> bool;

        /// Creates a new costing model from the given serialized [`crate::proto::Costing`] protobuf object.
        fn new_cost(costing: &[u8]) -> Result<SharedPtr<DynamicCost>>;
    }
}

// Safety: All operations do not mutate [`TileSet`] inner state.
unsafe impl Send for ffi::TileSet {}
unsafe impl Sync for ffi::TileSet {}

// Safety: All operations do not mutate [`GraphTile`] inner state.
unsafe impl Send for ffi::GraphTile {}
unsafe impl Sync for ffi::GraphTile {}

// Safety: All operations do not mutate [`DynamicCost`] inner state.
unsafe impl Send for ffi::DynamicCost {}
unsafe impl Sync for ffi::DynamicCost {}

impl Default for GraphId {
    fn default() -> Self {
        Self {
            // `valhalla::baldr::kInvalidGraphId`
            value: 0x3fffffffffff,
        }
    }
}

impl fmt::Debug for GraphId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphId")
            .field("level", &self.level())
            .field("tileid", &self.tileid())
            .field("id", &self.id())
            .finish()
    }
}

impl fmt::Display for GraphId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}/{}", self.level(), self.tileid(), self.id())
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

/// Represents errors returned by the Valhalla C++ API.
#[derive(Debug, Clone, PartialEq)]
pub struct Error(Box<str>);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<cxx::Exception> for Error {
    fn from(err: cxx::Exception) -> Self {
        Error(err.what().into())
    }
}

bitflags! {
    /// Access bit field constants. Access in directed edge allows 12 bits.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Access: u16 {
        const AUTO = 1;
        const PEDESTRIAN = 2;
        const BICYCLE = 4;
        const TRUCK = 8;
        const EMERGENCY = 16;
        const TAXI = 32;
        const BUS = 64;
        const HOV = 128;
        const WHEELCHAIR = 256;
        const MOPED = 512;
        const MOTORCYCLE = 1024;
        const ALL = 4095;
        const VEHICULAR = Self::AUTO.bits() | Self::TRUCK.bits() | Self::MOPED.bits() | Self::MOTORCYCLE.bits()
                        | Self::TAXI.bits() | Self::BUS.bits() | Self::HOV.bits();
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SpeedSources: u8 {
        /// Default edge speed - speed limit if available, otherwise typical speed for the edge type.
        const NO_FLOW = 0;
        /// Typical (average historical) speed during the night, from 7pm to 7am.
        const FREE_FLOW = 1;
        /// Typical (average historical) speed during the day, from 7am to 7pm.
        const CONSTRAINED_FLOW = 2;
        /// Historical traffic speed, stored in 5m buckets over the week.
        const PREDICTED_FLOW = 4;
        /// Live-traffic speed.
        const CURRENT_FLOW = 8;
        /// All available speed sources.
        const ALL = Self::FREE_FLOW.bits() | Self::CONSTRAINED_FLOW.bits()
                  | Self::PREDICTED_FLOW.bits() | Self::CURRENT_FLOW.bits();
    }
}

/// Coordinate in (lat, lon) format.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LatLon(pub f64, pub f64);

impl From<LatLon> for proto::LatLng {
    fn from(loc: LatLon) -> Self {
        proto::LatLng {
            has_lat: Some(proto::lat_lng::HasLat::Lat(loc.0)),
            has_lng: Some(proto::lat_lng::HasLng::Lng(loc.1)),
        }
    }
}

/// Handy wrapper as [`proto::Location`] has optional `ll` field that actually always should be set.
impl From<LatLon> for Option<proto::LatLng> {
    fn from(loc: LatLon) -> Self {
        Some(loc.into())
    }
}

/// High-level interface for reading Valhalla graph tiles from tar extracts.
///
/// As `GraphReader` already uses shared ownership internally, cloning is cheap and it can be
/// reused across threads without wrapping it in an [`Arc`].
///
/// N.B.: It is better to clone `GraphReader` instances rather than creating new ones from the same
/// configuration to avoid duplicate memory mappings (up to 80GB+ per instance for planetary tilesets).
#[derive(Clone)]
pub struct GraphReader(cxx::SharedPtr<ffi::TileSet>);

impl GraphReader {
    /// Creates a new GraphReader from the given Valhalla configuration, parsed into a [`Config`].
    ///
    /// # Examples
    ///
    /// ```
    /// let Ok(config) = valhalla::Config::from_file("path/to/config.json") else {
    ///     return; // Handle error appropriately
    /// };
    /// let reader = valhalla::GraphReader::new(&config);
    /// ```
    pub fn new(config: &Config) -> Result<Self, Error> {
        Ok(Self(ffi::new_tileset(config.inner())?))
    }

    /// Latest OSM changeset ID (or the maximum OSM Node/Way/Relation ID) in the OSM PBF file used to build the tileset.
    pub fn dataset_id(&self) -> u64 {
        self.0.dataset_id()
    }

    /// List all tiles in the tileset.
    pub fn tiles(&self) -> Vec<GraphId> {
        self.0.tiles()
    }

    /// List all tiles in the bounding box for a given hierarchy level in the tileset.
    pub fn tiles_in_bbox(&self, min: LatLon, max: LatLon, level: GraphLevel) -> Vec<GraphId> {
        self.0.tiles_in_bbox(
            min.0 as f32,
            min.1 as f32,
            max.0 as f32,
            max.1 as f32,
            level,
        )
    }

    /// Graph tile object at given GraphId if it exists in the tileset.
    #[deprecated(since = "0.6.9", note = "use `GraphReader::graph_tile()` instead")]
    pub fn get_tile(&self, id: GraphId) -> Option<GraphTile> {
        self.graph_tile(id)
    }

    /// Graph tile object at given GraphId if it exists in the tileset.
    #[deprecated(since = "0.6.11", note = "use `GraphReader::graph_tile()` instead")]
    pub fn tile(&self, id: GraphId) -> Option<GraphTile> {
        self.graph_tile(id)
    }

    /// Retrieves the graph tile data for a given [`GraphId`] if it exists in the tileset.
    pub fn graph_tile(&self, id: GraphId) -> Option<GraphTile> {
        GraphTile::new(self.0.get_graph_tile(id))
    }

    /// Retrieves the live traffic tile data for a given [`GraphId`] if it exists in the tileset.
    pub fn traffic_tile(&self, id: GraphId) -> Option<ffi::TrafficTile> {
        self.0.get_traffic_tile(id).ok()
    }
}

/// Graph information for a tile within the Tiled Hierarchical Graph.
///
/// `GraphTile` can outlive the [`GraphReader`] that created it.
/// As `GraphTile` already uses shared ownership internally, cloning is cheap and it can be
/// reused across threads without wrapping it in an [`Arc`].
#[derive(Clone)]
pub struct GraphTile(cxx::SharedPtr<ffi::GraphTile>);

impl GraphTile {
    fn new(tile: cxx::SharedPtr<ffi::GraphTile>) -> Option<Self> {
        if tile.is_null() {
            None
        } else {
            Some(Self(tile))
        }
    }

    /// GraphID of the tile, which includes the tile ID and hierarchy level.
    pub fn id(&self) -> GraphId {
        self.0.id()
    }

    /// Slice of all directed edges in the current tile.
    pub fn directededges(&self) -> &[ffi::DirectedEdge] {
        ffi::directededges(&self.0)
    }

    /// Gets a directed edge by index within the current tile.
    pub fn directededge(&self, index: u32) -> Option<&ffi::DirectedEdge> {
        match self.0.directededge(index as usize) {
            Ok(ptr) if !ptr.is_null() => Some(unsafe { &*ptr }),
            // Valhalla always return non-null ptr if ok and throws an exception if the index is out of bounds.
            // But it also sounds nice to handle nullptr in the same way.
            _ => None,
        }
    }

    /// Slice of all node in the current tile.
    pub fn nodes(&self) -> &[ffi::NodeInfo] {
        ffi::nodes(&self.0)
    }

    /// Gets a node by index within the current tile.
    pub fn node(&self, index: u32) -> Option<&ffi::NodeInfo> {
        match self.0.node(index as usize) {
            Ok(ptr) if !ptr.is_null() => Some(unsafe { &*ptr }),
            // Valhalla always return non-null ptr if ok and throws an exception if the index is out of bounds.
            // But it also sounds nice to handle nullptr in the same way.
            _ => None,
        }
    }

    /// Slice of all node transitions in the current tile.
    #[deprecated(
        since = "0.6.15",
        note = "please use `GraphTile::node_transitions()` instead"
    )]
    pub fn transitions(&self) -> &[ffi::NodeTransition] {
        ffi::transitions(&self.0)
    }

    /// Gets a node transition by index within the current tile.
    #[deprecated(
        since = "0.6.15",
        note = "please use `GraphTile::node_transitions()` instead"
    )]
    pub fn transition(&self, index: u32) -> Option<&ffi::NodeTransition> {
        match self.0.transition(index) {
            Ok(ptr) if !ptr.is_null() => Some(unsafe { &*ptr }),
            // Valhalla always return non-null ptr if ok and throws an exception if the index is out of bounds.
            // But it also sounds nice to handle nullptr in the same way.
            _ => None,
        }
    }

    /// Coordinate in (lat,lon) format for the given node.
    /// This gives the exact location of the node with better precision than [`EdgeInfo::shape`] start/end points.
    pub fn node_latlon(&self, node: &ffi::NodeInfo) -> LatLon {
        debug_assert!(ref_within_slice(self.nodes(), node), "Wrong tile");
        let latlon = ffi::node_latlon(&self.0, node);
        LatLon(latlon.lat, latlon.lon)
    }

    /// Slice of all outbound edges for the given node.
    pub fn node_edges(&self, node: &ffi::NodeInfo) -> &[ffi::DirectedEdge] {
        debug_assert!(ref_within_slice(self.nodes(), node), "Wrong tile");
        ffi::node_edges(&self.0, node)
    }

    /// Slice of all transitions to other hierarchy levels for the given node.
    pub fn node_transitions<'a>(&'a self, node: &ffi::NodeInfo) -> &'a [ffi::NodeTransition] {
        debug_assert!(ref_within_slice(self.nodes(), node), "Wrong tile");
        ffi::node_transitions(&self.0, node)
    }

    /// Information about the administrative area, such as country or state, by its index.
    /// Indices are stored in [`NodeInfo::admin_index()`] fields.
    pub fn admin_info(&self, index: u32) -> Option<ffi::AdminInfo> {
        ffi::admininfo(&self.0, index).ok()
    }

    /// Dynamic (cold) information about the edge, such as OSM Way ID, speed limit, shape, elevation, etc.
    pub fn edgeinfo(&self, de: &ffi::DirectedEdge) -> ffi::EdgeInfo {
        debug_assert!(ref_within_slice(self.directededges(), de), "Wrong tile");
        ffi::edgeinfo(&self.0, de)
    }

    /// Edge's live traffic speed in km/h if available. Returns `Some(0)` if the edge is closed due to traffic.
    pub fn live_speed(&self, de: &ffi::DirectedEdge) -> Option<u32> {
        debug_assert!(ref_within_slice(self.directededges(), de), "Wrong tile");
        match ffi::live_speed(&self.0, de) {
            0 => Some(0), // Edge is closed due to traffic
            255 => None,  // Live speed is unknown
            speed => Some(speed as u32),
        }
    }

    /// Convenience method to determine whether an edge is currently closed
    /// due to traffic. Roads are considered closed when the following are true
    ///   a) have traffic data for that tile
    ///   b) we have a valid record for that edge
    ///   b) the speed is zero
    pub fn edge_closed(&self, de: &ffi::DirectedEdge) -> bool {
        debug_assert!(ref_within_slice(self.directededges(), de), "Wrong tile");
        unsafe { self.0.IsClosed(de as *const ffi::DirectedEdge) }
    }

    /// Overall edge speed, mixed from different [`SpeedSources`] in km/h. As not all requested speed sources may be
    /// available for the edge, this function returns `(speed_kmh: u32, sources: SpeedSources)` tuple.
    ///
    /// This function never returns zero speed, even if the edge is closed due to traffic. [`GraphTile::edge_closed()`]
    ///  or [`GraphTile::live_speed()`] should be used to determine if the edge is closed instead.
    pub fn edge_speed(
        &self,
        de: &ffi::DirectedEdge,
        speed_sources: SpeedSources,
        is_truck: bool,
        second_of_week: u64,
        seconds_from_now: u64,
    ) -> (u32, SpeedSources) {
        debug_assert!(ref_within_slice(self.directededges(), de), "Wrong tile");
        let mut flow_sources: u8 = 0;
        let speed = unsafe {
            self.0.GetSpeed(
                de as *const ffi::DirectedEdge,
                speed_sources.bits(),
                second_of_week,
                is_truck,
                &mut flow_sources,
                seconds_from_now,
            )
        };
        (speed, SpeedSources::from_bits_retain(flow_sources))
    }
}

impl DirectedEdge {
    /// Access modes in the forward direction. Bit mask using [`Access`] constants.
    #[inline(always)]
    pub fn forwardaccess(&self) -> Access {
        Access::from_bits_retain(self.forwardaccess_u32() as u16)
    }

    /// Access modes in the reverse direction. Bit mask using [`Access`] constants.
    #[inline(always)]
    pub fn reverseaccess(&self) -> Access {
        Access::from_bits_retain(self.reverseaccess_u32() as u16)
    }
}

impl NodeInfo {
    /// Returns the range of edge indices for this node's outbound edges.
    ///
    /// This range can be used to slice the directed edges array from the same tile
    /// that contains this node. The range represents indices within the tile's
    /// edge array, not global edge identifiers.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn call_edges(reader: &valhalla::GraphReader) -> Option<()> {
    /// let node_id = valhalla::GraphId::from_parts(2, 12345, 67)?;
    ///
    /// // Get the tile containing the node
    /// let tile = reader.graph_tile(node_id.tile())?;
    /// let node = tile.node(node_id.id())?;
    ///
    /// for edge in &tile.directededges()[node.edges()] {
    ///     println!("- {node_id} -> {} edge has {} length", edge.endnode(), edge.length());
    /// }
    /// # Some(())
    /// # }
    /// ```
    #[deprecated(
        since = "0.6.10",
        note = "please use `GraphTile::node_edges()` instead"
    )]
    pub fn edges(&self) -> std::ops::Range<usize> {
        let start = self.edge_index() as usize;
        let count = self.edge_count() as usize;
        start..start + count
    }

    /// Returns the range of transition indices for this node's transitions to other hierarchy levels.
    ///
    /// This range can be used to slice the node transitions array from the same tile
    /// that contains this node. The range represents indices within the tile's
    /// node transitions array, not global transition identifiers.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn call_edges(reader: &valhalla::GraphReader) -> Option<()> {
    /// let node_id = valhalla::GraphId::from_parts(2, 12345, 67)?;
    ///
    /// // Get the tile containing the node
    /// let tile = reader.graph_tile(node_id.tile())?;
    /// let node = tile.node(node_id.id())?;
    ///
    /// for transition in &tile.transitions()[node.transitions()] {
    ///     println!("- {node_id} has a transition to the {} node", transition.endnode());
    /// }
    /// # Some(())
    /// # }
    /// ```
    #[deprecated(
        since = "0.6.10",
        note = "please use `GraphTile::node_transitions()` instead"
    )]
    pub fn transitions(&self) -> std::ops::Range<usize> {
        let start = self.transition_index() as usize;
        let count = self.transition_count() as usize;
        start..start + count
    }

    /// Access modes allowed to pass through the node. Bit mask using [`crate::Access`] constants.
    pub fn access(&self) -> Access {
        Access::from_bits_retain(self.access_u16())
    }
}

impl TimeZoneInfo {
    /// Retrieves the timezone information by its index if available. `unix_timestamp` is required to handle DST.
    pub fn from_id(id: u32, unix_timestamp: u64) -> Option<Self> {
        ffi::from_id(id, unix_timestamp).ok()
    }
}

/// Real-time traffic data for a single edge, including speeds, congestion levels, and incidents.
/// It is a Rust representation of `valhalla::baldr::TrafficSpeed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveTraffic(u64);

impl LiveTraffic {
    /// Live traffic data is unknown for the edge.
    pub const UNKNOWN: Self = Self(0);
    /// Edge is closed due to incident.
    pub const CLOSED: Self = Self(255u64 << 28); // set breakpoint1 to 255, keeping overall_encoded_speed at 0

    /// Constructs a `LiveTraffic` instance from its raw `u64` bit representation.
    /// The bit layout of the `u64` value must match the format of the
    /// [`valhalla::baldr::TrafficSpeed`] struct in the C++ Valhalla library.
    ///
    /// [`valhalla::baldr::TrafficSpeed`]: https://github.com/valhalla/valhalla/blob/master/valhalla/baldr/traffictile.h
    #[inline(always)]
    pub const fn from_bits(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw `u64` bit representation of the traffic data.
    /// The bit layout of the returned value is defined by the
    /// [`valhalla::baldr::TrafficSpeed`] struct in the C++ Valhalla library.
    ///
    /// [`valhalla::baldr::TrafficSpeed`]: https://github.com/valhalla/valhalla/blob/master/valhalla/baldr/traffictile.h
    #[inline(always)]
    pub const fn to_bits(&self) -> u64 {
        self.0
    }

    /// Creates traffic data from a single, uniform speed for the entire edge.
    /// Underlying segmented speeds are set to `[speed, 0, 0]` with breakpoints as `[255, 0]`.
    #[inline(always)]
    pub const fn from_uniform_speed(speed: u8) -> Self {
        Self::from_segmented_speeds(speed, [speed, 0, 0], [255, 0])
    }

    /// Creates traffic data from multiple speed values for different segments of the edge.
    #[inline(always)]
    pub const fn from_segmented_speeds(
        overall_speed: u8,
        subsegment_speeds: [u8; 3],
        breakpoints: [u8; 2],
    ) -> Self {
        let overall_encoded = (overall_speed >> 1) as u64;
        let speed1_encoded = (subsegment_speeds[0] >> 1) as u64;
        let speed2_encoded = (subsegment_speeds[1] >> 1) as u64;
        let speed3_encoded = (subsegment_speeds[2] >> 1) as u64;
        let bp1 = breakpoints[0] as u64;
        let bp2 = breakpoints[1] as u64;

        Self(
            overall_encoded |        // overall_encoded_speed at bit 0
            (speed1_encoded << 7) |  // encoded_speed1 at bit 7
            (speed2_encoded << 14) | // encoded_speed2 at bit 14
            (speed3_encoded << 21) | // encoded_speed3 at bit 21
            (bp1 << 28) |            // breakpoint1 at bit 28
            (bp2 << 36), // breakpoint2 at bit 36
        )
    }
}

impl TrafficTile {
    /// Live traffic information for the given edge index in the tile if available.
    pub fn edge_traffic(&self, edge_index: u32) -> Option<LiveTraffic> {
        match ffi::edge_traffic(self, edge_index) {
            Ok(data) => Some(LiveTraffic(data)),
            Err(_) => None,
        }
    }

    /// Writes live traffic information for the given edge index in the tile.
    pub fn write_edge_traffic(&self, edge_index: u32, traffic: LiveTraffic) {
        let _ = ffi::write_edge_traffic(self, edge_index, traffic.0);
    }
}

/// A [costing model] that evaluates edge traversal costs and accessibility for different travel modes
/// (auto, bicycle, pedestrian, etc.).
///
/// `CostingModel` wraps Valhalla's dynamic costing algorithms to determine whether edges and nodes
/// are accessible for a given travel mode, and to calculate the cost of traversing edges and
/// making turns at intersections. This enables graph traversal operations such as reachability
/// analysis, accessibility checking, and custom routing logic.
///
/// As `CostingModel` already uses shared ownership internally, cloning is cheap and it can be
/// reused across threads without wrapping it in an [`Arc`].
///
/// [costing model]: https://valhalla.github.io/valhalla/api/turn-by-turn/api-reference/#costing-models
#[derive(Clone)]
pub struct CostingModel(cxx::SharedPtr<ffi::DynamicCost>);

impl CostingModel {
    /// Creates a new costing model of the given type with default options.
    ///
    /// # Examples
    ///
    /// ```
    /// use valhalla::{CostingModel, proto};
    ///
    /// let cost_model = CostingModel::new(proto::costing::Type::Auto).unwrap();
    /// ```
    pub fn new(costing_type: proto::costing::Type) -> Result<Self, Error> {
        let costing = proto::Costing {
            r#type: costing_type as i32,
            ..Default::default()
        };
        let buf = costing.encode_to_vec();
        Ok(Self(ffi::new_cost(&buf)?))
    }

    /// Creates a new costing model with custom [costing options].
    ///
    /// [costing options]: https://valhalla.github.io/valhalla/api/turn-by-turn/api-reference/#costing-options
    ///
    /// # Examples
    ///
    /// ```
    /// use valhalla::{CostingModel, proto};
    ///
    /// let cost_model = CostingModel::with_options(&proto::Costing {
    ///     r#type: proto::costing::Type::Auto as i32,
    ///     has_options: Some(proto::costing::HasOptions::Options(
    ///         proto::costing::Options {
    ///             exclude_tolls: true,
    ///             exclude_ferries: true,
    ///             ..Default::default()
    ///         },
    ///     )),
    ///     ..Default::default()
    /// }).expect("Valid costing options");
    /// ```
    pub fn with_options(costing: &proto::Costing) -> Result<Self, Error> {
        let buf = costing.encode_to_vec();
        Ok(Self(ffi::new_cost(&buf)?))
    }

    /// Checks if the node is accessible according to this costing model.
    ///
    /// Node access can be restricted by bollards, gates, or access restrictions
    /// that are specific to the travel mode.
    pub fn node_accessible(&self, node: &ffi::NodeInfo) -> bool {
        unsafe { self.0.NodeAllowed(node as *const ffi::NodeInfo) }
    }

    /// Checks if the edge is accessible according to this costing model.
    ///
    /// This performs a basic accessibility check based on edge access permissions
    /// (auto/bicycle/pedestrian) without considering turn restrictions, closures,
    /// or routing-specific constraints.
    pub fn edge_accessible(&self, edge: &ffi::DirectedEdge) -> bool {
        unsafe { self.0.IsAccessible(edge as *const ffi::DirectedEdge) }
    }
}

/// Checks if the given reference points to an item within the given slice.
fn ref_within_slice<T>(slice: &[T], item: &T) -> bool {
    let start = slice.as_ptr() as usize;
    let item_pos = item as *const T as usize;
    let byte_offset = item_pos.wrapping_sub(start);
    byte_offset < std::mem::size_of_val(slice)
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
        assert_eq!(format!("{id}"), "2/838852/161285");
        assert_eq!(
            format!("{id:?}"),
            "GraphId { level: 2, tileid: 838852, id: 161285 }"
        );

        let base = id.tile();
        assert_eq!(base.level(), 2);
        assert_eq!(base.tileid(), 838852);
        assert_eq!(base.id(), 0);
        assert_eq!(GraphId::from_parts(id.level(), id.tileid(), 0), Some(base));

        let default_id = GraphId::default();
        assert_eq!(default_id.level(), 7);
        assert_eq!(default_id.tileid(), 4194303);
        assert_eq!(default_id.id(), 2097151);

        assert_eq!(GraphId::from_parts(8, id.tileid(), 0), None);
    }

    #[test]
    fn test_ref_within_slice() {
        let data = [10, 20, 30, 40, 50];
        assert!(ref_within_slice(&data, &data[0]));
        assert!(ref_within_slice(&data, &data[2]));
        assert!(ref_within_slice(&data, &data[4]));

        let outside = 30;
        assert!(!ref_within_slice(&data, &outside));

        let subslice = &data[1..4];
        assert!(!ref_within_slice(subslice, &data[0]));
        assert!(ref_within_slice(subslice, &data[1]));
        assert!(ref_within_slice(subslice, &data[2]));
        assert!(ref_within_slice(subslice, &data[3]));
        assert!(!ref_within_slice(subslice, &data[4]));
    }
}
