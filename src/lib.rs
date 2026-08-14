use std::{
    fmt,
    hash::{Hash, Hasher},
};

use bitflags::bitflags;
use cxx::ExternType;
#[cfg(feature = "proto")]
use prost::Message;

#[cfg(feature = "proto")]
mod actor;
pub mod config;
#[cfg(feature = "proto")]
pub mod proto;

#[cfg(feature = "proto")]
pub use actor::{Actor, Response};
pub use config::Config;
pub use config::ConfigBuilder;
pub use ffi::AdminInfo;
pub use ffi::EdgeInfo;
pub use ffi::EdgeUse;
pub use ffi::GraphLevel;
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

    /// Dynamic (cold) information about the edge, such as OSM Way ID, speed limit, shape, elevation, etc.
    struct EdgeInfo {
        /// OSM Way ID of the edge.
        way_id: u64,
        /// Speed limit in km/h. 0 if not available and 255 if not limited (e.g. autobahn).
        speed_limit: u8,
        /// polyline6 encoded shape of the edge.
        shape: String,
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

    /// An interface for reading and writing live traffic information for the corresponding graph tile.
    ///
    /// Can be obtained via [`crate::GraphReader::traffic_tile()`].
    /// `TrafficTile` can outlive the [`GraphReader`] that created it.
    struct TrafficTile {
        /// Pointer to [`valhalla::baldr::TrafficTileHeader`] of the tile.
        ///
        /// [`valhalla::baldr::TrafficTileHeader`]: https://github.com/valhalla/valhalla/blob/master/valhalla/baldr/traffictile.h
        header: *mut u64,
        /// Pointer to the start of the array of [`valhalla::baldr::TrafficSpeed`] records for the tile.
        ///
        /// [`valhalla::baldr::TrafficSpeed`]: https://github.com/valhalla/valhalla/blob/master/valhalla/baldr/traffictile.h
        speeds: *mut u64,
        /// Number of directed edges in the tile and thus number of `TrafficSpeed` records.
        edge_count: u32,
        /// Shared ownership of the underlying memory-mapped file with all traffic tiles.
        traffic_tar: SharedPtr<tar>,
    }

    // Force cxx to generate Vec<GraphId> support.
    impl Vec<GraphId> {}

    unsafe extern "C++" {
        include!("valhalla/src/libvalhalla.hpp");

        type GraphLevel;

        #[namespace = "valhalla::baldr"]
        type GraphId = crate::GraphId;
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
        // As cxx doesn't support `boost::intrusive_ptr<T>`, `GraphTile` lifetime should be manually
        // managed by calling [`ffi::clone()`] and [`ffi::drop()`].
        fn get_graph_tile(self: &TileSet, id: GraphId) -> *const GraphTile;
        fn get_traffic_tile(self: &TileSet, id: GraphId) -> Result<TrafficTile>;
        fn dataset_id(self: &TileSet) -> u64;

        #[namespace = "valhalla::baldr"]
        type GraphTile;
        // Clones pointer, increasing the ref counting.
        unsafe fn clone(tile: *const GraphTile) -> *const GraphTile;
        // Drops the pointer and decreases the ref count. `GraphTile` is deleted when ref_count reaches zero.
        unsafe fn drop(tile: *const GraphTile);
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
        unsafe fn GetSpeed(
            self: &GraphTile,
            de: *const DirectedEdge,
            flow_mask: u8,
            seconds: u64,
            is_truck: bool,
            flow_sources: *mut u8,
            seconds_from_now: u64,
        ) -> u32;
        // Helper method that returns the raw `TrafficSpeed` bits for the edge's live traffic record.
        fn live_traffic(tile: &GraphTile, de: &DirectedEdge) -> u64;

        #[namespace = "valhalla::midgard"]
        type tar;

        /// GraphID of the tile, which includes the tile ID and hierarchy level.
        fn id(tile: &TrafficTile) -> GraphId;
        /// Seconds since epoch of the last update.
        fn last_update(tile: &TrafficTile) -> u64;
        /// Writes the last update timestamp to the memory-mapped file.
        fn write_last_update(tile: &TrafficTile, unix_timestamp: u64);
        /// Custom spare value stored in the header.
        fn spare(tile: &TrafficTile) -> u64;
        /// Writes a custom value to the spare field in the memory-mapped file.
        fn write_spare(tile: &TrafficTile, spare: u64);

        #[namespace = "valhalla::baldr"]
        #[cxx_name = "Use"]
        type EdgeUse;

        #[namespace = "valhalla::baldr"]
        type RoadClass;

        #[namespace = "valhalla::baldr"]
        type DirectedEdge = crate::DirectedEdge;
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
        type NodeInfo = crate::NodeInfo;
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
        type NodeTransition = crate::NodeTransition;
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

    #[cfg(feature = "proto")]
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

// Safety: All operations do not mutate [`TileSet`] inner state and underlying resources are
// managed by C++ `std::shared_ptr`.
unsafe impl Send for ffi::TileSet {}
unsafe impl Sync for ffi::TileSet {}

// Safety: All operations do not mutate [`DynamicCost`] inner state.
#[cfg(feature = "proto")]
unsafe impl Send for ffi::DynamicCost {}
#[cfg(feature = "proto")]
unsafe impl Sync for ffi::DynamicCost {}

/// Identifier of a node or an edge within the tiled, hierarchical graph.
/// Includes the tile Id, hierarchy level, and a unique identifier within the tile/level.
#[derive(Clone, Copy, Eq)]
#[repr(C)]
pub struct GraphId {
    pub value: u64,
}

unsafe impl ExternType for GraphId {
    type Id = cxx::type_id!("valhalla::baldr::GraphId");
    type Kind = cxx::kind::Trivial;
}

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
    #[inline(always)]
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
    #[inline(always)]
    pub fn new(value: u64) -> Self {
        Self { value }
    }

    /// Constructs a new `GraphId` from the given hierarchy level, tile ID, and unique ID within the tile.
    /// Returns `None` if the level is invalid (greater than 7) or if the tile ID is invalid (greater than 2^22).
    #[inline(always)]
    pub fn from_parts(level: u32, tileid: u32, id: u32) -> Option<Self> {
        ffi::from_parts(level, tileid, id).ok()
    }

    /// Hierarchy level of the tile this identifier belongs to.
    #[inline(always)]
    pub fn level(&self) -> u32 {
        self.value as u32 & 0x7
    }

    /// Tile identifier of this GraphId within the hierarchy level.
    #[inline(always)]
    pub fn tileid(&self) -> u32 {
        ((self.value & 0x1fffff8) >> 3) as u32
    }

    /// Combined tile information (level and tile id) as a single value.
    #[inline(always)]
    pub fn tile(&self) -> GraphId {
        Self::new(self.value & 0x1ffffff)
    }

    /// Identifier within the tile, unique within the tile and level.
    #[inline(always)]
    pub fn id(&self) -> u32 {
        ((self.value & 0x3ffffe000000) >> 25) as u32
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
    ///
    /// Valhalla's [costing models] decide accessibility with these bits: a travel mode may use an
    /// edge or a node if any of its bits is set in [`DirectedEdge::forwardaccess()`] or
    /// [`NodeInfo::access()`].
    ///
    /// [costing models]: https://valhalla.github.io/valhalla/api/turn-by-turn/api-reference/#costing-models
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Access: u16 {
        const AUTO = 1;
        const PEDESTRIAN = 2;
        const BICYCLE = 4;
        const TRUCK = 8;
        const EMERGENCY = 16;
        const TAXI = 32;
        const BUS = 64;
        /// High-Occupancy Vehicle, i.e. carpool lanes marked via `hov=designated` or similar tags.
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

#[cfg(feature = "proto")]
impl From<LatLon> for proto::LatLng {
    fn from(loc: LatLon) -> Self {
        proto::LatLng {
            has_lat: Some(proto::lat_lng::HasLat::Lat(loc.0)),
            has_lng: Some(proto::lat_lng::HasLng::Lng(loc.1)),
        }
    }
}

/// Handy wrapper as [`proto::Location`] has optional `ll` field that actually always should be set.
#[cfg(feature = "proto")]
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
    /// let config = valhalla::ConfigBuilder {
    ///     mjolnir: valhalla::config::Mjolnir {
    ///         tile_extract: "path/to/tiles.tar".into(),
    ///         traffic_extract: "path/to/traffic.tar".into(), // optional
    ///         ..Default::default()
    ///     },
    ///     ..Default::default()
    /// }
    /// .build();
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
/// `GraphTile` uses manual reference counting via `boost::intrusive_ptr<T>` on the C++ side.
/// Cloning is cheap as it only increments the reference count.
///
/// **Thread Safety**: NOT Send/Sync due to non-atomic reference counting.
/// For multi-threaded use, call [`GraphReader::graph_tile()`] to have an access to the tile data
/// from each thread rather than sharing instances across threads. Consider caching instances for
/// faster graph traversal.
///
/// `GraphTile` can outlive the [`GraphReader`] that created it.
pub struct GraphTile(*const ffi::GraphTile);

impl Clone for GraphTile {
    fn clone(&self) -> Self {
        Self(unsafe { ffi::clone(self.0) })
    }
}

impl Drop for GraphTile {
    fn drop(&mut self) {
        unsafe { ffi::drop(self.0) };
    }
}

impl GraphTile {
    fn new(tile: *const ffi::GraphTile) -> Option<Self> {
        if !tile.is_null() {
            Some(Self(tile))
        } else {
            None
        }
    }

    /// Explicit implementation of [`std::ops::Deref`] to keep inner methods private.
    fn deref(&self) -> &ffi::GraphTile {
        // Safety: [`GraphTile::new()`] guarantees that inner pointer can't be null.
        unsafe { &*self.0 }
    }

    /// GraphID of the tile, which includes the tile ID and hierarchy level.
    #[inline(always)]
    pub fn id(&self) -> GraphId {
        self.deref().id()
    }

    /// Slice of all directed edges in the current tile.
    #[inline(always)]
    pub fn directededges(&self) -> &[ffi::DirectedEdge] {
        ffi::directededges(self.deref())
    }

    /// Gets a directed edge by index within the current tile.
    pub fn directededge(&self, index: u32) -> Option<&ffi::DirectedEdge> {
        match self.deref().directededge(index as usize) {
            Ok(ptr) if !ptr.is_null() => Some(unsafe { &*ptr }),
            // Valhalla always return non-null ptr if ok and throws an exception if the index is out of bounds.
            // But it also sounds nice to handle nullptr in the same way.
            _ => None,
        }
    }

    /// Slice of all node in the current tile.
    #[inline(always)]
    pub fn nodes(&self) -> &[ffi::NodeInfo] {
        ffi::nodes(self.deref())
    }

    /// Gets a node by index within the current tile.
    pub fn node(&self, index: u32) -> Option<&ffi::NodeInfo> {
        match self.deref().node(index as usize) {
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
        ffi::transitions(self.deref())
    }

    /// Gets a node transition by index within the current tile.
    #[deprecated(
        since = "0.6.15",
        note = "please use `GraphTile::node_transitions()` instead"
    )]
    pub fn transition(&self, index: u32) -> Option<&ffi::NodeTransition> {
        match self.deref().transition(index) {
            Ok(ptr) if !ptr.is_null() => Some(unsafe { &*ptr }),
            // Valhalla always return non-null ptr if ok and throws an exception if the index is out of bounds.
            // But it also sounds nice to handle nullptr in the same way.
            _ => None,
        }
    }

    /// Coordinate in (lat,lon) format for the given node.
    /// This gives the exact location of the node with better precision than [`EdgeInfo::shape`] start/end points.
    #[inline(always)]
    pub fn node_latlon(&self, node: &ffi::NodeInfo) -> LatLon {
        debug_assert!(ref_within_slice(self.nodes(), node), "Wrong tile");
        let latlon = ffi::node_latlon(self.deref(), node);
        LatLon(latlon.lat, latlon.lon)
    }

    /// Slice of all outbound edges for the given node.
    #[inline(always)]
    pub fn node_edges<'a>(&'a self, node: &ffi::NodeInfo) -> &'a [ffi::DirectedEdge] {
        debug_assert!(ref_within_slice(self.nodes(), node), "Wrong tile");
        ffi::node_edges(self.deref(), node)
    }

    /// Slice of all transitions to other hierarchy levels for the given node.
    #[inline(always)]
    pub fn node_transitions<'a>(&'a self, node: &ffi::NodeInfo) -> &'a [ffi::NodeTransition] {
        debug_assert!(ref_within_slice(self.nodes(), node), "Wrong tile");
        ffi::node_transitions(self.deref(), node)
    }

    /// Information about the administrative area, such as country or state, by its index.
    /// Indices are stored in [`NodeInfo::admin_index()`] fields.
    pub fn admin_info(&self, index: u32) -> Option<ffi::AdminInfo> {
        ffi::admininfo(self.deref(), index).ok()
    }

    /// Dynamic (cold) information about the edge, such as OSM Way ID, speed limit, shape, elevation, etc.
    #[inline(always)]
    pub fn edgeinfo(&self, de: &ffi::DirectedEdge) -> ffi::EdgeInfo {
        debug_assert!(ref_within_slice(self.directededges(), de), "Wrong tile");
        ffi::edgeinfo(self.deref(), de)
    }

    /// Live traffic record for this edge. When no traffic is loaded the record carries no reading
    /// ([`LiveTraffic::speed()`] returns `None`). Interpret via [`LiveTraffic::speed`] and [`LiveTraffic::segments`].
    #[inline(always)]
    pub fn live_traffic(&self, de: &ffi::DirectedEdge) -> LiveTraffic {
        debug_assert!(ref_within_slice(self.directededges(), de), "Wrong tile");
        LiveTraffic::from_bits(ffi::live_traffic(self.deref(), de))
    }

    /// Edge's live traffic speed in km/h if available. Returns `Some(0)` if the edge is closed due to traffic.
    #[deprecated(since = "0.6.41", note = "use `live_traffic(de).speed()` instead")]
    #[inline(always)]
    pub fn live_speed(&self, de: &ffi::DirectedEdge) -> Option<u32> {
        self.live_traffic(de).speed().map(u32::from)
    }

    /// Convenience method to determine whether an edge is currently closed
    /// due to traffic. Roads are considered closed when the following are true
    ///   a) have traffic data for that tile
    ///   b) we have a valid record for that edge
    ///   b) the speed is zero
    #[deprecated(
        since = "0.6.41",
        note = "use `live_traffic(de).speed() == Some(0)` instead"
    )]
    #[inline(always)]
    pub fn edge_closed(&self, de: &ffi::DirectedEdge) -> bool {
        self.live_traffic(de).speed() == Some(0)
    }

    /// Overall edge speed, mixed from different [`SpeedSources`] in km/h. As not all requested speed sources may be
    /// available for the edge, this function returns `(speed_kmh: u32, sources: SpeedSources)` tuple.
    ///
    /// This function never returns zero speed, even if the edge is closed due to traffic. Read the edge's live
    /// traffic via [`GraphTile::live_traffic()`] and check [`LiveTraffic::speed()`] for
    /// `Some(0)` to determine if the edge is closed instead.
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
            self.deref().GetSpeed(
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

/// Directed edge within the graph.
#[repr(C)]
pub struct DirectedEdge {
    // With this definition and cxx's magic it becomes possible to do pointer arithmetic properly,
    // allowing to operate with slices of `DirectedEdge` in Rust.
    // Otherwise, Rust compiler has no way to know the size of the `DirectedEdge` struct and assumes that
    // `DirectedEdge` is a zero-sized type (ZST), which leads to incorrect pointer arithmetic.
    // The whole Valhalla's ability to work with binary files (tilesets) relies this contract.
    data: [u64; 6],
}

unsafe impl ExternType for DirectedEdge {
    type Id = cxx::type_id!("valhalla::baldr::DirectedEdge");
    type Kind = cxx::kind::Trivial;
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

/// Information held for each node within the graph. The graph uses a forward star structure:
/// nodes point to the first outbound directed edge and each directed edge points to the other
/// end node of the edge.
#[repr(C)]
pub struct NodeInfo {
    // With this definition and cxx's magic it becomes possible to do pointer arithmetic properly,
    // allowing to operate with slices of `NodeInfo` in Rust.
    // Otherwise, Rust compiler has no way to know the size of the `NodeInfo` struct and assumes that
    // `NodeInfo` is a zero-sized type (ZST), which leads to incorrect pointer arithmetic.
    // The whole Valhalla's ability to work with binary files (tilesets) relies this contract.
    data: [u64; 4],
}

unsafe impl ExternType for NodeInfo {
    type Id = cxx::type_id!("valhalla::baldr::NodeInfo");
    type Kind = cxx::kind::Trivial;
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
    #[inline(always)]
    pub fn access(&self) -> Access {
        Access::from_bits_retain(self.access_u16())
    }
}

/// Records a transition between a node on the current tile and a node
/// at the same position on a different hierarchy level. Stores the GraphId
/// of the end node as well as a flag indicating whether the transition is
/// upwards (true) or downwards (false).
#[repr(C)]
pub struct NodeTransition {
    data: [u64; 1],
}

unsafe impl ExternType for NodeTransition {
    type Id = cxx::type_id!("valhalla::baldr::NodeTransition");
    type Kind = cxx::kind::Trivial;
}

impl TimeZoneInfo {
    /// Retrieves the timezone information by its index if available. `unix_timestamp` is required to handle DST.
    #[inline(always)]
    pub fn from_id(id: u32, unix_timestamp: u64) -> Option<Self> {
        ffi::from_id(id, unix_timestamp).ok()
    }
}

/// A contiguous portion of an edge with its own live-traffic reading. Yielded by
/// [`LiveTraffic::segments()`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrafficSegment {
    /// Portion of the edge this segment covers, as fractions of edge length `(start, end)`, `0.0..=1.0`.
    pub range: (f32, f32),
    /// Speed in km/h; `None` = no data for this portion (wins over congestion-closed), `Some(0)` =
    /// closed (also from congestion `1.0`). Guard `kph > 0` before dividing - closed is not "slow".
    pub speed: Option<u8>,
    /// Congestion level `0.0..=1.0`; `None` = unknown. `1.0` also folds `speed` to `Some(0)` (closed).
    pub congestion: Option<f32>,
}

/// Real-time traffic data for a single edge, including speeds, congestion levels, and incidents.
/// It is a Rust representation of `valhalla::baldr::TrafficSpeed`.
///
/// A `LiveTraffic` value is a *snapshot* - one volatile read of the record's 8 bytes - so all
/// accessors decode the same coherent bits even while a traffic updater rewrites the tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveTraffic(u64);

impl LiveTraffic {
    /// Live traffic data is unknown for the edge.
    pub const UNKNOWN: Self = Self(0);
    /// Edge is closed due to incident.
    pub const CLOSED: Self = Self(255u64 << 28); // set breakpoint1 to 255, keeping overall_encoded_speed at 0

    /// 7-bit speed value signaling "unknown". Mirrors `UNKNOWN_TRAFFIC_SPEED_RAW` in `traffictile.h`.
    const UNKNOWN_SPEED: u8 = (1 << 7) - 1;
    /// Max raw 6-bit congestion; `1..=63` maps to `[0.0, 1.0]`, `0` = unknown. Mirrors `MAX_CONGESTION_VAL` in `traffictile.h`.
    const MAX_CONGESTION: u8 = 63;

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
    /// Speeds floor to 2 km/h resolution; `speed >= 254` encodes the unknown sentinel (reads back `None`).
    #[inline(always)]
    pub const fn from_uniform_speed(speed: u8) -> Self {
        Self::from_segmented_speeds(speed, [speed, 0, 0], [255, 0])
    }

    /// Creates traffic data from multiple speed values for different segments of the edge.
    /// Speeds floor to 2 km/h (`>= 254` encodes the unknown sentinel). Breakpoints are `bp/255`
    /// fence fractions: `breakpoints[0] == 255` = uniform; `breakpoints[1] <= breakpoints[0]` truncates coverage.
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

    /// Sets the spare bit to the given value, returning a new `LiveTraffic` instance.
    #[inline(always)]
    pub const fn with_spare(self, spare: bool) -> Self {
        let cleared = self.0 & !(1u64 << 63); // Clear the spare bit
        let spare_bit = (spare as u64) << 63;
        Self(cleared | spare_bit)
    }

    /// Gets the value of the spare bit.
    #[inline(always)]
    pub const fn spare(&self) -> bool {
        self.0 & (1 << 63) != 0
    }

    /// Overall speed of the edge in km/h - the value Valhalla costing consumes; see
    /// [`LiveTraffic::segments()`] for per-segment detail. `None` = no live reading; `Some(0)` = closed.
    /// Guard `kph > 0` before dividing or threshold-comparing - closed is not "slow".
    ///
    /// ```
    /// use valhalla::LiveTraffic;
    /// assert_eq!(LiveTraffic::from_uniform_speed(72).speed(), Some(72));
    /// assert_eq!(LiveTraffic::UNKNOWN.speed(), None);
    /// ```
    #[inline(always)]
    pub const fn speed(&self) -> Option<u8> {
        let overall_encoded = (self.0 & 0x7F) as u8; // bits 0..=6
        let breakpoint1 = ((self.0 >> 28) & 0xFF) as u8; // bits 28..=35
        if breakpoint1 == 0 || overall_encoded == Self::UNKNOWN_SPEED {
            None
        } else {
            Some(overall_encoded << 1)
        }
    }

    /// Live-traffic detail as 1-3 contiguous [`TrafficSegment`]s in edge direction; a uniform record
    /// yields one segment `(0.0, 1.0)`. Empty when [`LiveTraffic::speed()`] is `None`; coverage may
    /// end before the edge does.
    #[inline(always)]
    pub fn segments(&self) -> TrafficSegments {
        TrafficSegments {
            bits: self.0,
            index: 0,
        }
    }

    /// Whether the edge references incidents in the corresponding incident tile.
    /// Meaningful even without a speed reading.
    #[inline(always)]
    pub const fn has_incidents(&self) -> bool {
        self.0 & (1 << 62) != 0
    }

    /// Sets the per-segment congestion (`[0.0, 1.0]` clamped, `None` = unknown), preserving all other
    /// bits; read back via [`TrafficSegment::congestion`]. Note: `Some(1.0)` is the wire's *closed*
    /// marker - that segment reads back with `speed == Some(0)` (unless its speed is unknown - `None` wins).
    #[inline(always)]
    pub fn with_congestion(self, congestion: [Option<f32>; 3]) -> Self {
        // Clear the three 6-bit congestion fields (bits 44..=61), preserving everything else.
        let cleared = self.0 & !(0x3_FFFFu64 << 44);
        let c1 = encode_congestion(congestion[0]);
        let c2 = encode_congestion(congestion[1]);
        let c3 = encode_congestion(congestion[2]);
        Self(cleared | (c1 << 44) | (c2 << 50) | (c3 << 56))
    }

    /// Sets or clears the incidents bit, preserving all other bits. Referencing a real incident
    /// tile is the caller's responsibility.
    #[inline(always)]
    pub const fn with_incidents(self, has_incidents: bool) -> Self {
        let cleared = self.0 & !(1u64 << 62); // Clear the incidents bit
        let incidents_bit = (has_incidents as u64) << 62;
        Self(cleared | incidents_bit)
    }
}

/// Iterator over the 1-3 [`TrafficSegment`]s of a [`LiveTraffic`] record.
/// Decodes lazily from a copied `u64` - no allocation, detached from the memory-mapped tile.
#[derive(Clone, Debug)]
pub struct TrafficSegments {
    /// Raw record bits, copied at [`LiveTraffic::segments()`] time.
    bits: u64,
    /// Next segment index to yield.
    index: u8,
}

impl Iterator for TrafficSegments {
    type Item = TrafficSegment;

    fn next(&mut self) -> Option<TrafficSegment> {
        let segment = decode_segment(self.bits, self.index)?;
        self.index += 1;
        Some(segment)
    }
}

impl std::iter::FusedIterator for TrafficSegments {}

/// Decodes segment `i` of a record, or `None` when it does not exist - the single place that knows
/// the segment layout. Segment 0 exists iff the record has a reading; segments exist only while the
/// breakpoint fences strictly advance, which uniformly handles uniform records (`breakpoint1 == 255`),
/// truncated coverage (`breakpoint2 <= breakpoint1`), and garbage encodings.
fn decode_segment(bits: u64, i: u8) -> Option<TrafficSegment> {
    if i >= 3 || LiveTraffic::from_bits(bits).speed().is_none() {
        return None;
    }
    let breakpoint1 = ((bits >> 28) & 0xFF) as u8; // bits 28..=35
    let breakpoint2 = ((bits >> 36) & 0xFF) as u8; // bits 36..=43
    let fences = [
        (0, breakpoint1),
        (breakpoint1, breakpoint2),
        (breakpoint2, 255),
    ];
    if fences[..=i as usize]
        .iter()
        .any(|(start, end)| end <= start)
    {
        return None;
    }
    let (start, end) = fences[i as usize];
    let speed_raw = ((bits >> (7 + 7 * i)) & 0x7F) as u8; // encoded_speed{1,2,3}
    let congestion_raw = ((bits >> (44 + 6 * i)) & 0x3F) as u8; // congestion{1,2,3}
    Some(TrafficSegment {
        range: (start as f32 / 255.0, end as f32 / 255.0),
        speed: decode_segment_speed(speed_raw, congestion_raw),
        congestion: decode_congestion(congestion_raw as u64),
    })
}

/// Decodes a segment's raw 7-bit speed and raw 6-bit congestion pair into its reading:
/// - `None` when the speed holds the UNKNOWN sentinel (`127`) - partial coverage. Unknown speed
///   wins over congestion-closed (unlike C++ `closed(subsegment)`, which reports sentinel + raw-63 as closed).
/// - `Some(0)` (closed) when the congestion is raw `63` - the C++ `closed(subsegment)` semantic.
/// - `Some(speed_raw << 1)` otherwise - a raw speed of `0` yields `Some(0)` (closed) naturally,
///   the wire's own encoding of closure.
#[inline(always)]
const fn decode_segment_speed(speed_raw: u8, congestion_raw: u8) -> Option<u8> {
    if speed_raw == LiveTraffic::UNKNOWN_SPEED {
        None
    } else if congestion_raw == LiveTraffic::MAX_CONGESTION {
        Some(0)
    } else {
        Some(speed_raw << 1)
    }
}

/// Decodes a 6-bit raw congestion value into a normalized `[0.0, 1.0]` fraction, or `None` when
/// unknown (raw `0`).
#[inline(always)]
fn decode_congestion(raw: u64) -> Option<f32> {
    if raw == 0 {
        None
    } else {
        Some((raw as f32 - 1.0) / (LiveTraffic::MAX_CONGESTION as f32 - 1.0))
    }
}

/// Encodes a normalized `[0.0, 1.0]` congestion fraction into its 6-bit raw value, the inverse of
/// [`decode_congestion`]. `None` and non-finite values map to raw `0` (unknown); finite `Some(f)`
/// clamps to `[0.0, 1.0]` and maps to `round(f * 62) + 1` in `1..=63`.
#[inline(always)]
fn encode_congestion(congestion: Option<f32>) -> u64 {
    match congestion {
        Some(f) if f.is_finite() => {
            (f.clamp(0.0, 1.0) * (LiveTraffic::MAX_CONGESTION as f32 - 1.0)).round() as u64 + 1
        }
        _ => 0,
    }
}

impl TrafficTile {
    /// GraphID of the tile, which includes the tile ID and hierarchy level.
    #[inline(always)]
    pub fn id(&self) -> GraphId {
        ffi::id(self)
    }

    /// Seconds since epoch of the last update.
    #[inline(always)]
    pub fn last_update(&self) -> u64 {
        ffi::last_update(self)
    }

    /// Writes the last update timestamp to the memory-mapped file.
    #[inline(always)]
    pub fn write_last_update(&self, unix_timestamp: u64) {
        ffi::write_last_update(self, unix_timestamp)
    }

    /// Custom spare value stored in the header.
    #[inline(always)]
    pub fn spare(&self) -> u64 {
        ffi::spare(self)
    }

    /// Writes a custom value to the spare field in the memory-mapped file.
    #[inline(always)]
    pub fn write_spare(&self, spare: u64) {
        ffi::write_spare(self, spare)
    }

    /// Number of directed edges in this traffic tile.
    #[inline(always)]
    pub fn edge_count(&self) -> u32 {
        self.edge_count
    }

    /// Live traffic information for the given edge index in the tile if available.
    #[inline(always)]
    pub fn edge_traffic(&self, edge_index: u32) -> Option<LiveTraffic> {
        if edge_index < self.edge_count {
            let data = unsafe { std::ptr::read_volatile(self.speeds.add(edge_index as usize)) };
            Some(LiveTraffic(data))
        } else {
            None
        }
    }

    /// Writes live traffic information for the given edge index in the tile.
    #[inline(always)]
    pub fn write_edge_traffic(&self, edge_index: u32, traffic: LiveTraffic) {
        if edge_index < self.edge_count {
            unsafe { std::ptr::write_volatile(self.speeds.add(edge_index as usize), traffic.0) };
        }
    }

    /// Clears live traffic information in the tile and sets the last update time to 0.
    /// The spare field is left unchanged.
    pub fn clear_traffic(&self) {
        for i in 0..self.edge_count as usize {
            unsafe { std::ptr::write_volatile(self.speeds.add(i), 0u64) };
        }
        self.write_last_update(0);
    }
}

/// A [costing model] that decides which edges and nodes a travel mode may use.
///
/// # Deprecated
///
/// Both checks it exposes are plain [`Access`] bit tests, and the options given here skip
/// Valhalla's own defaults: unset fields stay at protobuf zero, so `truck` also accepts `AUTO`
/// edges. See [`Access`] for the bits every other costing uses.
///
/// | Deprecated | Replacement for `auto` |
/// |---|---|
/// | `costing.node_accessible(node)` | `node.access().intersects(Access::AUTO \| Access::HOV)` |
/// | `costing.edge_accessible(edge)` | `edge.forwardaccess().intersects(Access::AUTO \| Access::HOV)` |
///
/// Edge costs, turn costs, restrictions and closures were never exposed and would drag in a large
/// part of Valhalla's internals - use the [`Actor`] API for those.
///
/// [costing model]: https://valhalla.github.io/valhalla/api/turn-by-turn/api-reference/#costing-models
#[cfg(feature = "proto")]
#[deprecated(
    since = "0.6.43",
    note = "use `Access` bits from `DirectedEdge::forwardaccess()` and `NodeInfo::access()` instead"
)]
#[derive(Clone)]
pub struct CostingModel(cxx::SharedPtr<ffi::DynamicCost>);

#[cfg(feature = "proto")]
#[allow(deprecated)]
impl CostingModel {
    /// Creates a new costing model of the given type with default options.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![allow(deprecated)]
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
    /// # #![allow(deprecated)]
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

    #[test]
    fn test_mock_live_traffic_tile() {
        let mut header: [u64; 16] = [0; 16]; // it should be just big enough. The exact header size is 32 bytes.
        let mut speeds: [u64; 16] = [0; 16];
        let tile = TrafficTile {
            header: header.as_mut_ptr(),
            speeds: speeds.as_mut_ptr(),
            edge_count: 16,
            traffic_tar: cxx::SharedPtr::null(),
        };

        // Initial state with all zeros
        assert_eq!(tile.last_update(), 0);
        assert_eq!(tile.spare(), 0);
        assert_eq!(tile.edge_count(), 16);
        for i in 0..tile.edge_count() {
            assert_eq!(tile.edge_traffic(i), Some(LiveTraffic::UNKNOWN));
        }

        // Out-of-range access
        assert_eq!(tile.edge_traffic(tile.edge_count()), None);
        assert_eq!(tile.edge_traffic(u32::MAX), None);
        let before = speeds;
        tile.write_edge_traffic(tile.edge_count(), LiveTraffic::from_uniform_speed(200));
        tile.write_edge_traffic(u32::MAX, LiveTraffic::from_uniform_speed(200));
        assert_eq!(speeds, before, "out-of-range write must be a no-op");

        // Let's mutate stuff
        tile.write_last_update(1234567890);
        tile.write_spare(42);
        for i in 0..tile.edge_count() {
            tile.write_edge_traffic(i, LiveTraffic::from_uniform_speed(i as u8));
        }

        assert_eq!(tile.last_update(), 1234567890);
        assert_eq!(tile.spare(), 42);
        for i in 0..tile.edge_count() {
            assert_eq!(
                tile.edge_traffic(i),
                Some(LiveTraffic::from_uniform_speed(i as u8))
            );
        }

        // Each speed is just a u64, encoded in a specific way. This checks that there is no weirdness in that area.
        for i in 0..tile.edge_count() {
            assert_eq!(
                speeds[i as usize],
                LiveTraffic::from_uniform_speed(i as u8).to_bits()
            );
        }

        // The high bits (congestion 44..=61, incidents 62, spare 63) must survive the raw volatile
        // round-trip too, not just the low 44 bits written by `from_uniform_speed`.
        let full = LiveTraffic::from_segmented_speeds(72, [60, 80, 100], [127, 200])
            .with_congestion([Some(0.5), None, Some(1.0)])
            .with_incidents(true)
            .with_spare(true);
        tile.write_edge_traffic(3, full);
        assert_eq!(tile.edge_traffic(3), Some(full));
        assert_eq!(speeds[3], full.to_bits());
        // Setting incidents (62) and spare (63) means the record occupies the high half of the u64.
        assert!(full.has_incidents());
        assert!(full.spare());
        assert_ne!(full.to_bits() >> 32, 0, "high 32 bits must be exercised");
    }

    #[test]
    fn live_traffic_speed() {
        use pretty_assertions::assert_eq;

        // UNKNOWN sentinel record carries no reading (breakpoint1 == 0).
        assert_eq!(LiveTraffic::UNKNOWN.speed(), None);
        // CLOSED record: breakpoint1 == 255, overall_encoded == 0 -> Some(0), the wire's own
        // encoding of closure.
        assert_eq!(LiveTraffic::CLOSED.speed(), Some(0));

        // Uniform known speed.
        assert_eq!(LiveTraffic::from_uniform_speed(72).speed(), Some(72));

        // Segmented known speed uses the overall speed only.
        assert_eq!(
            LiveTraffic::from_segmented_speeds(72, [60, 80, 100], [127, 200]).speed(),
            Some(72)
        );

        // The C++ INVALID_SPEED record (overall + all three subsegment speeds == 127,
        // breakpoints == 0) carries no reading.
        assert_eq!(LiveTraffic::from_bits(0x0FFF_FFFF).speed(), None);

        // The 127 sentinel gates on its own: no reading even with a nonzero breakpoint.
        let invalid = LiveTraffic::from_bits(
            (LiveTraffic::UNKNOWN_SPEED as u64) // overall_encoded == 127
            | (255u64 << 28), // breakpoint1 != 0
        );
        assert_eq!(invalid.speed(), None);

        // Precedence: `breakpoint1 == 0` gates first. An all-zero record has no reading - it is
        // NOT closed (`Some(0)`), even though its overall_encoded is 0.
        assert_eq!(LiveTraffic::from_bits(0).speed(), None);
        // A nonzero overall speed with `breakpoint1 == 0` still has no reading (no valid coverage).
        let no_breakpoint = LiveTraffic::from_bits(36); // overall_encoded == 36, breakpoint1 == 0
        assert_eq!(no_breakpoint.speed(), None);
    }

    #[test]
    fn live_traffic_segments() {
        use pretty_assertions::assert_eq;

        let segment = |range, speed, congestion| TrafficSegment {
            range,
            speed,
            congestion,
        };

        // No reading -> no segments (UNKNOWN and the C++ INVALID_SPEED record alike).
        assert_eq!(LiveTraffic::UNKNOWN.segments().count(), 0);
        assert_eq!(LiveTraffic::from_bits(0x0FFF_FFFF).segments().count(), 0);

        // CLOSED and uniform records are not special cases - each is exactly one segment
        // covering the whole edge.
        assert_eq!(
            LiveTraffic::CLOSED.segments().collect::<Vec<_>>(),
            [segment((0.0, 1.0), Some(0), None)]
        );
        assert_eq!(
            LiveTraffic::from_uniform_speed(72)
                .segments()
                .collect::<Vec<_>>(),
            [segment((0.0, 1.0), Some(72), None)]
        );

        // Segmented record exercising all three per-segment states: moving, closed (speed 0),
        // and no-data (254 kph in -> the encoded 127 UNKNOWN sentinel).
        let (bp1, bp2) = (100.0 / 255.0, 200.0 / 255.0);
        assert_eq!(
            LiveTraffic::from_segmented_speeds(72, [50, 0, 254], [100, 200])
                .segments()
                .collect::<Vec<_>>(),
            [
                segment((0.0, bp1), Some(50), None),
                segment((bp1, bp2), Some(0), None),
                segment((bp2, 1.0), None, None),
            ]
        );

        // All three subsegments unknown while the overall speed is known: the overall summary
        // stays authoritative.
        let all_unknown = LiveTraffic::from_segmented_speeds(72, [254, 254, 254], [100, 200]);
        assert_eq!(all_unknown.speed(), Some(72));
        assert!(
            all_unknown
                .segments()
                .all(|segment| segment.speed.is_none())
        );
        assert_eq!(all_unknown.segments().count(), 3);

        // Fence boundaries: breakpoint2 == 255 ends the record at two segments; breakpoint2 <=
        // breakpoint1 (0, equal, or garbage out-of-order) truncates coverage to a single segment.
        let two = LiveTraffic::from_segmented_speeds(72, [60, 80, 100], [127, 255]);
        assert_eq!(
            two.segments().collect::<Vec<_>>(),
            [
                segment((0.0, 127.0 / 255.0), Some(60), None),
                segment((127.0 / 255.0, 1.0), Some(80), None),
            ]
        );
        for breakpoints in [[127, 0], [100, 80], [100, 100]] {
            let truncated = LiveTraffic::from_segmented_speeds(72, [60, 80, 100], breakpoints);
            assert_eq!(
                truncated.segments().collect::<Vec<_>>(),
                [segment(
                    (0.0, breakpoints[0] as f32 / 255.0),
                    Some(60),
                    None
                )],
                "breakpoints {breakpoints:?}"
            );
        }
        assert_eq!(
            LiveTraffic::from_segmented_speeds(72, [60, 80, 100], [255, 255])
                .segments()
                .collect::<Vec<_>>(),
            [segment((0.0, 1.0), Some(60), None)]
        );

        // Congestion: raw 0 / 1 / 63 -> None / Some(0.0) / Some(1.0) per 6-bit field, and raw 63
        // (== written 1.0) also folds that segment's speed to Some(0) = closed (C++
        // `closed(subsegment)`) - unless the segment's speed is unknown, which wins.
        let bits = LiveTraffic::from_segmented_speeds(72, [60, 80, 100], [100, 200]).to_bits();
        let record = LiveTraffic::from_bits(
            bits | (1u64 << 44) | ((LiveTraffic::MAX_CONGESTION as u64) << 56),
        );
        assert_eq!(
            record.segments().collect::<Vec<_>>(),
            [
                segment((0.0, bp1), Some(60), Some(0.0)),
                segment((bp1, bp2), Some(80), None),
                segment((bp2, 1.0), Some(0), Some(1.0)),
            ]
        );
        let sentinel_and_congested =
            LiveTraffic::from_segmented_speeds(72, [254, 50, 50], [100, 200]).with_congestion([
                Some(1.0),
                None,
                None,
            ]);
        assert_eq!(
            sentinel_and_congested.segments().next().unwrap(),
            segment((0.0, bp1), None, Some(1.0))
        );

        // with_congestion round-trip: quantized to 6 bits (step 1/62), so only exactly-representable
        // values (endpoints and 0.5) survive `==`; out-of-range clamps; non-finite is not a reading.
        let base = LiveTraffic::from_segmented_speeds(72, [60, 80, 100], [100, 200]);
        let congestion = |t: LiveTraffic| {
            t.segments()
                .map(|segment| segment.congestion)
                .collect::<Vec<_>>()
        };
        let with = base.with_congestion([None, Some(0.0), Some(0.5)]);
        assert_eq!(congestion(with), [None, Some(0.0), Some(0.5)]);
        let clamped = base.with_congestion([Some(-1.0), Some(2.0), None]);
        assert_eq!(congestion(clamped), [Some(0.0), Some(1.0), None]);
        let non_finite =
            base.with_congestion([Some(f32::NAN), Some(f32::INFINITY), Some(f32::NEG_INFINITY)]);
        assert_eq!(congestion(non_finite), [None, None, None]);

        // with_congestion only touches the congestion bits (44..=61): speed, ranges, incidents and
        // spare are preserved, and clearing back to all-None restores the original bits exactly.
        let record = base.with_incidents(true).with_spare(true);
        let with = record.with_congestion([Some(0.25), Some(0.75), None]);
        assert_eq!(with.speed(), record.speed());
        for (w, r) in with.segments().zip(record.segments()) {
            assert_eq!(w.range, r.range);
            assert_eq!(w.speed, r.speed);
        }
        assert!(with.has_incidents() && with.spare());
        assert_eq!(with.with_congestion([None, None, None]), record);
    }

    #[test]
    fn live_traffic_incidents() {
        use pretty_assertions::assert_eq;

        // Bit 62 reads via has_incidents(); the spare bit (63) must not be mistaken for it.
        assert!(!LiveTraffic::UNKNOWN.has_incidents());
        assert!(LiveTraffic::from_bits(1u64 << 62).has_incidents());
        assert!(!LiveTraffic::UNKNOWN.with_spare(true).has_incidents());

        let record = LiveTraffic::from_uniform_speed(72)
            .with_congestion([Some(0.5), None, Some(1.0)])
            .with_spare(true);

        let set = record.with_incidents(true);
        assert!(set.has_incidents());
        // Speed, segments (including their congestion) and spare are untouched.
        assert_eq!(set.speed(), record.speed());
        assert_eq!(
            set.segments().collect::<Vec<_>>(),
            record.segments().collect::<Vec<_>>()
        );
        assert!(set.spare());

        // Clearing the incidents bit restores the original record exactly.
        let cleared = set.with_incidents(false);
        assert!(!cleared.has_incidents());
        assert_eq!(cleared, record);
    }
}
