//! Dijkstra over the raw road graph, with the caller supplying the rules.
//! Cost is path length in metres: it orders the queue, `Found` reports it, `max_distance` caps it.
//! No hierarchy limits — `max_distance` is the only cutoff.

use rustc_hash::FxHashMap;
use valhalla::breaking::{GraphReader, TileId};
use valhalla::{DirectedEdge, GraphId, NodeInfo};

use crate::{bitset::BitSet, priority_queue::PriorityQueue};

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
    graph_reader: &mut impl GraphReader,
    edges: &[GraphId],
    node_filter: impl Fn(&NodeInfo) -> bool,
    edge_filter: impl Fn(&DirectedEdge) -> bool,
    stop_condition: impl Fn(&DirectedEdge) -> bool,
    max_distance: u32,
) -> SearchResult {
    let mut nodes_to_visit = PriorityQueue::<u32, GraphId>::new();
    // One bitset per tile, allocated fresh per search.
    let mut visited = FxHashMap::<GraphId, BitSet>::default();

    // Start from the far end of each snapped edge.
    for edge in edges {
        let Some(tile) = graph_reader.graph_tile(TileId::of(*edge)) else {
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
        let Some(tile) = graph_reader.graph_tile(TileId::of(node_id)) else {
            continue; // incomplete tileset, skip this node
        };
        let first_visit = visited
            .entry(node_id.tile())
            .or_insert_with(|| BitSet::new(tile.nodes().len()))
            .insert(node_id.id() as usize);
        if !first_visit {
            continue; // already visited
        }

        if path_length >= max_distance {
            return SearchResult::MaxDistanceReached;
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

        for de in tile.node_edges(node) {
            if !edge_filter(de) {
                continue;
            }
            if stop_condition(de) {
                return SearchResult::Found(path_length);
            }

            // Skip the push/pop entirely for nodes already settled.
            let next_node = de.endnode();
            if visited
                .get(&next_node.tile())
                .is_some_and(|set| set.contains(next_node.id() as usize))
            {
                continue;
            }
            nodes_to_visit.push(path_length + de.length(), next_node);
        }
    }

    SearchResult::Exhausted
}
