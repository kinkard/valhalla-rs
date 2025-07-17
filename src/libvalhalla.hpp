#pragma once

#include <valhalla/baldr/graphtile.h>
#include <boost/property_tree/ptree_fwd.hpp>

#include "cxx.h"

namespace valhalla::midgard {
struct tar;
}

struct DirectedEdgeSlice;
struct EdgeInfo;
struct NodeInfoSlice;
struct TimeZoneInfo;

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

  std::unordered_map<uint64_t, std::pair<char*, size_t>> tiles_;
  std::unordered_map<uint64_t, std::pair<char*, size_t>> traffic_tiles_;
  std::shared_ptr<valhalla::midgard::tar> tar_;
  std::shared_ptr<valhalla::midgard::tar> traffic_tar_;
  uint64_t checksum_;

  rust::Vec<valhalla::baldr::GraphId> tiles() const;
  rust::Vec<valhalla::baldr::GraphId> tiles_in_bbox(float min_lat, float min_lon, float max_lat, float max_lon,
                                                    GraphLevel level) const;
  valhalla::baldr::graph_tile_ptr get_tile(valhalla::baldr::GraphId id) const;
  uint64_t dataset_id() const;
};

/// Creates a new [`TileSet`] instance based on a Valhalla's config.
std::shared_ptr<TileSet> new_tileset(const boost::property_tree::ptree& config);

/// Helper function as cxx unable to call constructors with arguments.
inline valhalla::baldr::GraphId from_parts(uint32_t level, uint32_t tileid, uint32_t id) {
  return valhalla::baldr::GraphId(tileid, level, id);
}

/// The workaround to use `SharedPtr<GraphTile>` in Rust because of the `graph_tile_ptr` defined as
/// `std::shared_ptr<const GraphTile>` and `cxx` doesn't support `const` in `SharedPtr`.
using GraphTile = const valhalla::baldr::GraphTile;

/// Helper function that allows to iterate over a slice of directed edges of that tile in Rust
DirectedEdgeSlice directededges(const GraphTile& tile);

/// Helper function that allows to iterate over a slice of nodes of that tile in Rust
NodeInfoSlice nodes(const GraphTile& tile);

/// Helper function that workarounds the inability to use `baldr::EdgeInfo` in Rust
EdgeInfo edgeinfo(const GraphTile& tile, const valhalla::baldr::DirectedEdge& de);

/// Helper method that returns 0 if the edge is closed, 255 if live speed in unknown and speed in km/h otherwise
uint8_t live_speed(const GraphTile& tile, const valhalla::baldr::DirectedEdge& de);

/// Helper function to resolve tz name and offset from a given id and unix timestamp.
TimeZoneInfo from_id(uint32_t id, uint64_t unix_timestamp);
