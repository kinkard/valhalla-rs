use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::ops::ControlFlow;
use std::path::Path;

use crate::{GraphLevel, GraphTile, LatLon, TrafficTile};

use super::{
    TileArchive, TileId, graph_tile_from_dir, graph_tile_from_dir_with_traffic, tile_file_suffix,
    tile_id_from_path,
};

/// Source of graph tiles.
///
/// `graph_tile` takes `&mut self`, as C++'s `GetGraphTile` is non-const, so a caching reader needs
/// no interior mutability. Readers are cheap to clone: give each thread its own.
pub trait GraphReader {
    /// `None` both for a tile the source does not hold and for one it fails to load.
    fn graph_tile(&mut self, id: TileId) -> Option<GraphTile>;

    /// Every tile the source holds, in unspecified order.
    fn tiles(&self) -> Vec<TileId>;

    /// The tiles the source holds among those a bounding box touches. Checks presence without
    /// loading; see [`TileId::covering`] for the geometry alone.
    fn tiles_in_bbox(&self, min: LatLon, max: LatLon, level: GraphLevel) -> Vec<TileId>;

    /// Latest OSM changeset id the tileset was built from. Loads a tile to read its header.
    fn dataset_id(&self) -> u64;
}

/// Live traffic tiles out of a `traffic.tar`, independent of any graph source.
#[derive(Clone)]
pub struct TrafficExtract(TileArchive);

impl TrafficExtract {
    pub fn open(path: &str) -> Result<Self, crate::Error> {
        TileArchive::open_traffic(path).map(Self)
    }

    /// Writable mapping, required by the [`TrafficTile`] write methods.
    pub fn open_writable(path: &str) -> Result<Self, crate::Error> {
        TileArchive::open_traffic_writable(path).map(Self)
    }

    pub fn tile(&self, id: TileId) -> Option<TrafficTile> {
        self.0.traffic_tile(id)
    }

    pub fn tiles(&self) -> Vec<TileId> {
        self.0.tiles()
    }
}

/// Graph tiles out of a `tiles.tar`, optionally joined with live traffic.
#[derive(Clone)]
pub struct TileExtract {
    graph: TileArchive,
    traffic: Option<TrafficExtract>,
}

impl TileExtract {
    pub fn open(path: &str) -> Result<Self, crate::Error> {
        Ok(Self {
            graph: TileArchive::open_graph(path)?,
            traffic: None,
        })
    }

    /// Opens `mjolnir.tile_extract`, joining `mjolnir.traffic_extract` when it can be opened.
    ///
    /// Traffic is best-effort because `ConfigBuilder` defaults it to a path that need not exist.
    pub fn from_config(config: &crate::Config) -> Result<Self, crate::Error> {
        let extract = config
            .get_str("mjolnir.tile_extract")
            .ok_or_else(|| crate::Error::msg("config names no mjolnir.tile_extract"))?;
        let reader = Self::open(&extract)?;
        let traffic = config
            .get_str("mjolnir.traffic_extract")
            .and_then(|path| TrafficExtract::open(&path).ok());
        Ok(match traffic {
            Some(traffic) => reader.with_traffic(traffic),
            None => reader,
        })
    }

    pub fn with_traffic(mut self, traffic: TrafficExtract) -> Self {
        self.traffic = Some(traffic);
        self
    }
}

impl GraphReader for TileExtract {
    fn graph_tile(&mut self, id: TileId) -> Option<GraphTile> {
        match &self.traffic {
            Some(traffic) => self.graph.graph_tile_with_traffic(id, &traffic.0),
            None => self.graph.graph_tile(id),
        }
    }
    fn tiles(&self) -> Vec<TileId> {
        self.graph.tiles()
    }
    fn tiles_in_bbox(&self, min: LatLon, max: LatLon, level: GraphLevel) -> Vec<TileId> {
        self.graph.tiles_in_bbox(min, max, level)
    }
    fn dataset_id(&self) -> u64 {
        self.graph.dataset_id()
    }
}

/// Graph tiles out of a `tile_dir`, optionally joined with live traffic.
#[derive(Clone)]
pub struct TileDir {
    dir: String,
    traffic: Option<TrafficExtract>,
}

impl TileDir {
    pub fn new(dir: impl Into<String>) -> Self {
        Self {
            dir: dir.into(),
            traffic: None,
        }
    }

    pub fn with_traffic(mut self, traffic: TrafficExtract) -> Self {
        self.traffic = Some(traffic);
        self
    }

    fn load(&self, id: TileId) -> Option<GraphTile> {
        match &self.traffic {
            Some(traffic) => graph_tile_from_dir_with_traffic(&self.dir, id, &traffic.0),
            None => graph_tile_from_dir(&self.dir, id),
        }
    }

    /// Visits every tile file under the hierarchy's level directories, the way C++'s
    /// `GraphReader::GetTileSet` does: levels 0 to 3 (transit), files and symlinks, anything that
    /// does not parse as a tile path skipped. Paths are parsed relative to the root, so a parent
    /// directory with an all-digit name cannot leak into the id.
    fn for_each_tile(&self, mut visit: impl FnMut(TileId) -> ControlFlow<()>) {
        fn walk(
            root: &Path,
            dir: &Path,
            visit: &mut dyn FnMut(TileId) -> ControlFlow<()>,
        ) -> ControlFlow<()> {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return ControlFlow::Continue(());
            };
            for entry in entries.flatten() {
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                let path = entry.path();
                if kind.is_dir() {
                    walk(root, &path, visit)?;
                } else if kind.is_file() || kind.is_symlink() {
                    let relative = path.strip_prefix(root).unwrap_or(&path);
                    if let Ok(id) = tile_id_from_path(&relative.to_string_lossy()) {
                        visit(id)?;
                    }
                }
            }
            ControlFlow::Continue(())
        }

        let root = Path::new(&self.dir);
        for level in 0..=3 {
            if walk(root, &root.join(level.to_string()), &mut visit).is_break() {
                return;
            }
        }
    }
}

impl GraphReader for TileDir {
    fn graph_tile(&mut self, id: TileId) -> Option<GraphTile> {
        self.load(id)
    }

    /// Walks the whole directory tree.
    fn tiles(&self) -> Vec<TileId> {
        let mut tiles = Vec::new();
        self.for_each_tile(|id| {
            tiles.push(id);
            ControlFlow::Continue(())
        });
        tiles
    }

    /// One `stat` per candidate. A gzipped tile counts, since the loader reads those too.
    fn tiles_in_bbox(&self, min: LatLon, max: LatLon, level: GraphLevel) -> Vec<TileId> {
        let root = Path::new(&self.dir);
        TileId::covering(min, max, level)
            .into_iter()
            .filter(|id| {
                root.join(tile_file_suffix(*id, false)).is_file()
                    || root.join(tile_file_suffix(*id, true)).is_file()
            })
            .collect()
    }

    /// Stops walking at the first tile that loads.
    fn dataset_id(&self) -> u64 {
        let mut dataset_id = 0;
        self.for_each_tile(|id| match self.load(id) {
            Some(tile) => {
                dataset_id = tile.dataset_id();
                ControlFlow::Break(())
            }
            None => ControlFlow::Continue(()),
        });
        dataset_id
    }
}

/// Unbounded tile cache over a reader. Never evicts, and caches misses too, so a search that keeps
/// walking off the edge of the tileset does not re-ask the reader.
pub struct Cached<R> {
    reader: R,
    tiles: HashMap<TileId, Option<GraphTile>>,
}

impl<R: GraphReader> Cached<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            tiles: HashMap::new(),
        }
    }

    pub fn cached_tiles(&self) -> usize {
        self.tiles.len()
    }

    pub fn clear(&mut self) {
        self.tiles.clear();
    }

    pub fn reader(&self) -> &R {
        &self.reader
    }

    /// Start and end node of an edge. Loads up to two more tiles, hence on the cache.
    pub fn edge_nodes(
        &mut self,
        edge_id: crate::GraphId,
    ) -> Option<(crate::GraphId, crate::GraphId)> {
        let tile = self.graph_tile(TileId::of(edge_id))?;
        let edge = tile.directededge(edge_id.id())?;
        let end_node_id = edge.endnode();

        let end_tile = self.graph_tile(TileId::of(end_node_id))?;
        let end_node = end_tile.node(end_node_id.id())?;
        let opp_edge = end_tile
            .node_edges(end_node)
            .get(edge.opp_index() as usize)?;

        Some((opp_edge.endnode(), end_node_id))
    }
}

impl<R: GraphReader> GraphReader for Cached<R> {
    fn graph_tile(&mut self, id: TileId) -> Option<GraphTile> {
        match self.tiles.entry(id) {
            Entry::Occupied(hit) => hit.get().clone(),
            Entry::Vacant(miss) => miss.insert(self.reader.graph_tile(id)).clone(),
        }
    }
    fn tiles(&self) -> Vec<TileId> {
        self.reader.tiles()
    }
    fn tiles_in_bbox(&self, min: LatLon, max: LatLon, level: GraphLevel) -> Vec<TileId> {
        self.reader.tiles_in_bbox(min, max, level)
    }
    fn dataset_id(&self) -> u64 {
        self.reader.dataset_id()
    }
}
