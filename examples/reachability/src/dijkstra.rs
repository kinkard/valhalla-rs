//! Dijkstra over the raw road graph, with the caller supplying the rules.
//! Cost is path length in metres: it orders the queue, `Found` reports it, `max_distance` caps it.
//! No hierarchy limits — `max_distance` is the only cutoff.

use std::collections::hash_map::Entry;

use rustc_hash::FxHashMap;
use valhalla::{DirectedEdge, GraphId, GraphReader, GraphTile, NodeInfo};

use crate::{bitset::BitSet, priority_queue::PriorityQueue};

/// One edge of a restored path, with the nodes it runs between.
#[derive(Debug, Clone, Copy)]
pub struct PathStep {
    pub from: GraphId,
    pub edge: GraphId,
    pub to: GraphId,
}

/// A finished search: its outcome, plus enough breadcrumbs to walk the path back.
pub struct Search {
    pub result: SearchResult,
    /// Node -> the node it was first reached from, and the edge that got there.
    came_from: FxHashMap<GraphId, (GraphId, GraphId)>,
    /// Where `stop_condition` fired: the node it fired at and the edge that satisfied it.
    found: Option<(GraphId, GraphId)>,
}

impl Search {
    /// Edges from the origin to where the search stopped, in travel order.
    pub fn path(&self) -> Vec<PathStep> {
        let Some((last_node, last_edge)) = self.found else {
            return Vec::new();
        };

        // Walk the breadcrumbs back to the origin, then flip into travel order.
        let mut steps = vec![PathStep {
            from: last_node,
            edge: last_edge,
            to: GraphId::default(),
        }];
        let mut node = last_node;
        while let Some(&(from, edge)) = self.came_from.get(&node) {
            steps.push(PathStep {
                from,
                edge,
                to: node,
            });
            node = from;
        }
        steps.reverse();
        steps
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchResult {
    /// An edge satisfying `stop_condition` was reached, at this distance in metres.
    Found(u32),
    /// The reachable graph ran out before `max_distance` was spent.
    Exhausted,
    /// `max_distance` was reached without satisfying `stop_condition`.
    MaxDistanceReached,
}

/// Explores outward from `edges`, following only what the filters allow.
pub fn search(
    graph_reader: &mut CachedGraphReader,
    edges: &[GraphId],
    node_filter: impl Fn(&NodeInfo) -> bool,
    edge_filter: impl Fn(&DirectedEdge) -> bool,
    stop_condition: impl Fn(&DirectedEdge) -> bool,
    max_distance: u32,
) -> Search {
    let mut nodes_to_visit = PriorityQueue::<u32, GraphId>::new();
    let mut came_from = FxHashMap::<GraphId, (GraphId, GraphId)>::default();
    // One bitset per tile, allocated fresh per search.
    let mut visited = FxHashMap::<GraphId, BitSet>::default();

    // Start from the far end of each snapped edge.
    for edge in edges {
        let Some(tile) = graph_reader.graph_tile(*edge) else {
            continue;
        };
        let Some(de) = tile.directededge(edge.id()) else {
            continue;
        };
        if edge_filter(de) {
            nodes_to_visit.push(0, de.endnode());
        }
    }

    while let Some((path_length, node_id)) = nodes_to_visit.pop() {
        // First pop of a node is its shortest path; later arrivals are dropped.
        let tile = match visited.entry(node_id.tile()) {
            Entry::Occupied(mut occupied) => {
                if !occupied.get_mut().insert(node_id.id() as usize) {
                    continue; // already visited
                }
                graph_reader
                    .graph_tile(node_id)
                    .expect("tile was loaded when it was added to `visited`")
            }
            Entry::Vacant(vacant) => {
                let Some(tile) = graph_reader.graph_tile(node_id) else {
                    continue; // incomplete tileset, skip this node
                };
                let node_count = tile.nodes().len();
                vacant
                    .insert(BitSet::new(node_count))
                    .insert(node_id.id() as usize);
                tile
            }
        };

        if path_length >= max_distance {
            return Search {
                result: SearchResult::MaxDistanceReached,
                came_from,
                found: None,
            };
        }

        let node = tile
            .node(node_id.id())
            .expect("node id came from this tile");
        if !node_filter(node) {
            continue;
        }

        // Changing hierarchy level covers no ground, so transitions keep the same cost.
        for transition in tile.node_transitions(node) {
            nodes_to_visit.push(path_length, transition.endnode());
        }

        for (i, de) in tile.node_edges(node).iter().enumerate() {
            if !edge_filter(de) {
                continue;
            }
            // An edge is identified by its index in the tile, counting from the node's first.
            let edge_id = GraphId::from_parts(
                node_id.level(),
                node_id.tileid(),
                node.edge_index() + i as u32,
            )
            .expect("edge index came from this tile");

            if stop_condition(de) {
                return Search {
                    result: SearchResult::Found(path_length),
                    came_from,
                    found: Some((node_id, edge_id)),
                };
            }

            // Skip the push/pop entirely for nodes already settled.
            let next_node = de.endnode();
            if visited
                .get(&next_node.tile())
                .is_some_and(|set| set.contains(next_node.id() as usize))
            {
                continue;
            }
            came_from.entry(next_node).or_insert((node_id, edge_id));
            nodes_to_visit.push(path_length + de.length(), next_node);
        }
    }

    Search {
        result: SearchResult::Exhausted,
        came_from,
        found: None,
    }
}

/// Tile cache shared across every search from one origin. Nothing is evicted.
pub struct CachedGraphReader {
    graph_reader: GraphReader,
    tiles: FxHashMap<GraphId, GraphTile>,
}

impl CachedGraphReader {
    pub fn new(graph_reader: GraphReader) -> Self {
        Self {
            graph_reader,
            tiles: Default::default(),
        }
    }

    pub fn graph_tile(&mut self, graph_id: GraphId) -> Option<&GraphTile> {
        let tile_id = graph_id.tile();
        match self.tiles.entry(tile_id) {
            Entry::Occupied(occupied) => Some(occupied.into_mut()),
            Entry::Vacant(vacant) => Some(vacant.insert(self.graph_reader.graph_tile(tile_id)?)),
        }
    }
}
