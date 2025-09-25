use valhalla::{Config, CostingModel, GraphReader, proto};

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

    let auto = CostingModel::new(proto::costing::Type::Auto).unwrap();
    let pedestrian = CostingModel::new(proto::costing::Type::Pedestrian).unwrap();
    for &edge in &edges {
        let de = tile.directededge(edge).unwrap();
        let node = tile.node(de.endnode().id()).unwrap();

        assert!(auto.edge_accessible(de));
        assert!(auto.node_allowed(node));

        // Pedestrians can't go through that tunnel.
        assert!(!pedestrian.edge_accessible(de));
        // But they can access the nodes.
        assert!(pedestrian.node_allowed(node));
    }
}
