//! Next breaking release, staged. Each item here replaces its top-level namesake when it lands -
//! [`GraphReader`] the concrete [`crate::GraphReader`] first. Not covered by semver until then.

use crate::{Error, GraphId, GraphLevel, GraphTile, LatLon, TrafficTile, ffi};

mod reader;

pub use reader::*;

/// Identifies a tile, as opposed to a node or an edge within one.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TileId(u32);

impl TileId {
    /// Tile a [`GraphId`] belongs to.
    pub fn of(id: GraphId) -> Self {
        Self(id.level() | (id.tileid() << 3))
    }

    pub fn new(level: GraphLevel, tileid: u32) -> Self {
        Self(level.repr as u32 | (tileid << 3))
    }

    pub fn level(self) -> GraphLevel {
        GraphLevel {
            repr: (self.0 & 0x7) as u8,
        }
    }

    pub fn tileid(self) -> u32 {
        self.0 >> 3
    }

    /// Every tile of the hierarchy a bounding box touches, whether or not anything was built there.
    /// Pure geometry, for tiles a reader does not hold yet; [`GraphReader::tiles_in_bbox`] is the
    /// one that lists what a reader has.
    pub fn covering(min: LatLon, max: LatLon, level: GraphLevel) -> Vec<Self> {
        ffi::tile_ids_in_bbox(
            min.0 as f32,
            min.1 as f32,
            max.0 as f32,
            max.1 as f32,
            level,
        )
        .into_iter()
        .map(Self::of)
        .collect()
    }

    /// The [`GraphId`] of this tile's base.
    pub fn graph_id(self) -> GraphId {
        GraphId::new(self.0 as u64)
    }
}

impl From<GraphId> for TileId {
    fn from(id: GraphId) -> Self {
        Self::of(id)
    }
}

impl std::fmt::Debug for TileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TileId({}/{})", self.level().repr, self.tileid())
    }
}

/// One memory-mapped tar of tiles and its index. Graph and traffic tars share the layout, so the
/// caller decides which one it is. Cheap to clone; `Send + Sync`.
#[derive(Clone)]
pub struct TileArchive(cxx::SharedPtr<ffi::TileArchive>);

impl TileArchive {
    /// Opens a tar of graph tiles, mapped read-only.
    pub fn open_graph(path: &str) -> Result<Self, Error> {
        Ok(Self(ffi::open_graph_archive(path)?))
    }

    /// Opens a tar of traffic tiles, mapped read-only.
    pub fn open_traffic(path: &str) -> Result<Self, Error> {
        Ok(Self(ffi::open_traffic_archive(path, true)?))
    }

    /// Opens a tar of traffic tiles, mapped writable.
    pub fn open_traffic_writable(path: &str) -> Result<Self, Error> {
        Ok(Self(ffi::open_traffic_archive(path, false)?))
    }

    pub fn tiles(&self) -> Vec<TileId> {
        self.0.tiles().into_iter().map(TileId::of).collect()
    }

    pub fn tiles_in_bbox(&self, min: LatLon, max: LatLon, level: GraphLevel) -> Vec<TileId> {
        self.0
            .tiles_in_bbox(
                min.0 as f32,
                min.1 as f32,
                max.0 as f32,
                max.1 as f32,
                level,
            )
            .into_iter()
            .map(TileId::of)
            .collect()
    }

    /// Loads the first tile to read its header.
    pub fn dataset_id(&self) -> u64 {
        self.0.dataset_id()
    }

    /// Reads this archive as graph tiles.
    pub fn graph_tile(&self, id: TileId) -> Option<GraphTile> {
        GraphTile::new(self.0.graph_tile(id.graph_id()))
    }

    /// Reads this archive as graph tiles, joining live traffic out of a second archive.
    pub fn graph_tile_with_traffic(&self, id: TileId, traffic: &TileArchive) -> Option<GraphTile> {
        GraphTile::new(self.0.graph_tile_with_traffic(id.graph_id(), &traffic.0))
    }

    /// Reads this archive as traffic tiles.
    pub fn traffic_tile(&self, id: TileId) -> Option<TrafficTile> {
        self.0.traffic_tile(id.graph_id()).ok()
    }
}

/// Reads one graph tile out of a `tile_dir` layout.
pub fn graph_tile_from_dir(dir: &str, id: TileId) -> Option<GraphTile> {
    GraphTile::new(ffi::graph_tile_from_dir(dir, id.graph_id()).ok()?)
}

/// Reads one graph tile out of a `tile_dir` layout, joining live traffic out of an archive.
pub fn graph_tile_from_dir_with_traffic(
    dir: &str,
    id: TileId,
    traffic: &TileArchive,
) -> Option<GraphTile> {
    GraphTile::new(ffi::graph_tile_from_dir_with_traffic(dir, id.graph_id(), &traffic.0).ok()?)
}

/// Reads one graph tile out of a `tile_dir` by memory-mapping the file, rather than reading it
/// into a fresh buffer as Valhalla does. Uncompressed tiles only.
pub fn graph_tile_from_dir_mmap(dir: &str, id: TileId) -> Option<GraphTile> {
    GraphTile::new(ffi::graph_tile_from_dir_mmap(dir, id.graph_id()).ok()?)
}

/// As [`graph_tile_from_dir_mmap`], joining live traffic out of an archive.
pub fn graph_tile_from_dir_mmap_with_traffic(
    dir: &str,
    id: TileId,
    traffic: &TileArchive,
) -> Option<GraphTile> {
    GraphTile::new(ffi::graph_tile_from_dir_mmap_with_traffic(dir, id.graph_id(), &traffic.0).ok()?)
}

/// Builds one graph tile over `bytes` without copying them. The tile owns the buffer from here on
/// and frees it when its last clone drops, or right away if construction fails.
///
/// Tile contents are trusted, as on every other path: only the header's size, end offset and
/// version are checked. Returns `None` when those fail, and when the header names a tile other
/// than `id` - a check `GraphTile::Create` itself does not make.
pub fn graph_tile_from_memory(id: TileId, bytes: Vec<u8>) -> Option<GraphTile> {
    let tile = GraphTile::new(ffi::graph_tile_from_memory(id.graph_id(), bytes).ok()?)?;
    (TileId::of(tile.id()) == id).then_some(tile)
}

/// As [`graph_tile_from_memory`], joining live traffic out of an archive.
pub fn graph_tile_from_memory_with_traffic(
    id: TileId,
    bytes: Vec<u8>,
    traffic: &TileArchive,
) -> Option<GraphTile> {
    let tile = GraphTile::new(
        ffi::graph_tile_from_memory_with_traffic(id.graph_id(), bytes, &traffic.0).ok()?,
    )?;
    (TileId::of(tile.id()) == id).then_some(tile)
}

/// Copies `bytes` into a tile. Prefer [`graph_tile_from_memory`] when the buffer can be handed over.
pub fn graph_tile_from_bytes(id: TileId, bytes: &[u8]) -> Option<GraphTile> {
    graph_tile_from_memory(id, bytes.to_vec())
}

/// The `tile_dir`-relative path a tile is stored at, e.g. `2/000/519/120.gph`.
pub fn tile_file_suffix(id: TileId, gzipped: bool) -> String {
    ffi::tile_file_suffix(id.graph_id(), gzipped)
}

/// Inverse of [`tile_file_suffix`].
pub fn tile_id_from_path(path: &str) -> Result<TileId, Error> {
    Ok(TileId::of(ffi::tile_id_from_path(path)?))
}
