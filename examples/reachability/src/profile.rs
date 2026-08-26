//! Elevation along a restored path, resampled at a fixed spacing.

use valhalla::GraphId;

use crate::dijkstra::{CachedGraphReader, PathStep};

/// Elevation of a node, or `None` when the tileset carries none.
fn node_elevation(cache: &mut CachedGraphReader, node: GraphId) -> Option<f32> {
    let tile = cache.graph_tile(node)?;
    let elevation = tile.node(node.id())?.elevation();
    (elevation != valhalla::NO_ELEVATION).then_some(elevation)
}

/// Elevation every `step` metres along the path, starting at its first node.
///
/// Tiles sample at their own spacing, so those are interpolated onto ours. Only output allocates.
pub fn resample(cache: &mut CachedGraphReader, path: &[PathStep], step: f32) -> Vec<f32> {
    let Some(first) = path.first() else {
        return Vec::new();
    };
    let Some(mut previous) = node_elevation(cache, first.from) else {
        return Vec::new();
    };

    let mut resampled = vec![previous];
    // Distance from the last emitted sample to `previous`, which sits at `previous_at`.
    let mut previous_at = 0.0;
    let mut emitted_at = 0.0;

    for path_step in path {
        let Some(end) = node_elevation(cache, path_step.to) else {
            break;
        };
        let Some(tile) = cache.graph_tile(path_step.edge) else {
            break;
        };
        let Some(edge) = tile.directededge(path_step.edge.id()) else {
            break;
        };

        // Samples sit between the end nodes at even spacing, so the count gives their positions.
        let samples = tile.edgeinfo(edge).elevation(edge, previous, end);
        let spacing = edge.length() as f32 / (samples.len() + 1) as f32;

        for elevation in samples.chain(std::iter::once(end)) {
            let at = previous_at + spacing;
            // Emit every `step` boundary this segment crosses, interpolating as we go.
            while emitted_at + step <= at {
                emitted_at += step;
                let along = (emitted_at - previous_at) / (at - previous_at);
                resampled.push(previous + (elevation - previous) * along);
            }
            (previous, previous_at) = (elevation, at);
        }
    }
    resampled
}
