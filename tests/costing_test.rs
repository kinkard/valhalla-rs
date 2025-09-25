use pretty_assertions::assert_eq;
use std::collections::{HashMap, VecDeque};

use valhalla::{Config, CostingModel, GraphId, GraphReader, proto};

const ANDORRA_TILES: &str = "tests/andorra/tiles.tar";

#[test]
fn costing_model() {
    let reader = GraphReader::new(&Config::from_tile_extract(ANDORRA_TILES).unwrap())
        .expect("Failed to create GraphReader");

    // There is only one toll road in Andorra - Túnel d'Envalira, OSM Way 6176755. Let's find it.
    let (tile, edges) = reader
        .tiles()
        .into_iter()
        .find_map(|tile_id| {
            let tile = reader.get_tile(tile_id).unwrap();
            let toll_edges = tile
                .directededges()
                .iter()
                .enumerate()
                .filter_map(|(i, de)| {
                    (de.toll() && tile.edgeinfo(de).way_id == 6176755).then_some(i as u32)
                })
                .collect::<Vec<_>>();
            (!toll_edges.is_empty()).then_some((tile, toll_edges))
        })
        .unwrap();

    // Test function that finds the furthest reachable node from the given node
    let furthest_node_distance =
        |reader: &GraphReader, costing: &CostingModel, node_id: GraphId| {
            let mut tile_id = node_id.tile();
            let mut tile = reader.get_tile(tile_id).unwrap();

            // labels keep the shortest distance to each node we've found so far
            let mut node_labels = HashMap::new();
            node_labels.insert(node_id, 0u32);
            let mut to_visit = VecDeque::new();
            to_visit.push_back(node_id);

            while let Some(node_id) = to_visit.pop_back() {
                if node_id.tile() != tile_id {
                    tile_id = node_id.tile();
                    tile = reader.get_tile(tile_id).unwrap();
                }
                let node = tile.node(node_id.id()).unwrap();
                if !costing.node_accessible(node) {
                    continue;
                }

                let curr_length = *node_labels.get(&node_id).unwrap();
                for de in &tile.directededges()[node.edges()] {
                    if !costing.edge_accessible(de) {
                        continue;
                    }

                    let next_node_id = de.endnode();
                    let next_length = curr_length + de.length();
                    if node_labels
                        .get(&next_node_id)
                        .is_none_or(|&length| next_length < length)
                    {
                        node_labels.insert(next_node_id, next_length);
                        to_visit.push_back(next_node_id);
                    }
                }
            }
            node_labels.into_values().max().unwrap()
        };

    assert_eq!(edges.len(), 2);
    // these two nodes are at the ends of the tunnel
    let first_node = tile.directededge(edges[0]).unwrap().endnode();
    let second_node = tile.directededge(edges[1]).unwrap().endnode();

    let auto = CostingModel::new(proto::costing::Type::Auto).unwrap();
    assert_eq!(furthest_node_distance(&reader, &auto, first_node), 48626);
    assert_eq!(furthest_node_distance(&reader, &auto, second_node), 45679);

    let bike = CostingModel::new(proto::costing::Type::Bicycle).unwrap();
    assert_eq!(furthest_node_distance(&reader, &bike, first_node), 0); // yep, somehow it is possible to reach tunnel entrance, but not go through it
    assert_eq!(furthest_node_distance(&reader, &bike, second_node), 45679);

    let pedestrian = CostingModel::new(proto::costing::Type::Pedestrian).unwrap();
    assert_eq!(furthest_node_distance(&reader, &pedestrian, first_node), 0); // no luck for pedestrians either
    assert_eq!(furthest_node_distance(&reader, &pedestrian, second_node), 0);
}
