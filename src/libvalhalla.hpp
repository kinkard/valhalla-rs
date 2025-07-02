#pragma once

#include <valhalla/baldr/graphtile.h>

#include "cxx.h"

#include <memory>
#include <unordered_map>

namespace valhalla::midgard {
struct tar;
}

struct DirectedEdgeSlice;
struct EdgeInfo;
struct TrafficEdge;

enum class GraphLevel : uint8_t {
  Highway = 0,
  Arterial = 1,
  Local = 2,
};

/// Exposed internal [`valhalla::baldr::GraphReader::tile_extract_t`], used to
/// access exact graph and traffic tiles. Create it using [`new_tileset()`].
struct TileSet {
  /// Explicitly define destructor as otherwise compiler will fail with
  /// std::unique_ptr due to forward declarations for `midgard::tar`
  ~TileSet();

  std::unordered_map<uint64_t, std::pair<char*, size_t>> tiles;
  std::unordered_map<uint64_t, std::pair<char*, size_t>> traffic_tiles;
  std::shared_ptr<valhalla::midgard::tar> archive;
  std::shared_ptr<valhalla::midgard::tar> traffic_archive;
  uint64_t checksum;

  rust::Vec<valhalla::baldr::GraphId> tiles_in_bbox(float min_lat, float min_lon, float max_lat, float max_lon,
                                                    GraphLevel level) const;
  valhalla::baldr::graph_tile_ptr get_tile(valhalla::baldr::GraphId id) const;
};

/// Creates a new [`TileSet`] instance based on a Valhalla's config json file
std::shared_ptr<TileSet> new_tileset(const std::string& config_file);

/// The workaround to use `SharedPtr<GraphTile>` in Rust because of the `graph_tile_ptr` defined as
/// `std::shared_ptr<const GraphTile>` and `cxx` doesn't support `const` in `SharedPtr`.
using GraphTile = const valhalla::baldr::GraphTile;

/// Helper function that allows to iterate over a slice of directed edges of that tile in Rust
DirectedEdgeSlice directededges(const GraphTile& tile);

/// Helper function that workarounds the inability to use `baldr::EdgeInfo` in Rust
EdgeInfo edgeinfo(const GraphTile& tile, const valhalla::baldr::DirectedEdge& de);

/// Retrieves all traffic flows for a given tile.
/// todo: move it in Rust and implement via bindings
rust::Vec<TrafficEdge> get_tile_traffic_flows(const GraphTile& tile);
