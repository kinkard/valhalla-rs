#pragma once

#include "cxx.h"

#include <memory>
#include <unordered_map>

namespace valhalla::midgard {
struct tar;
}

/// Use primitive type instead of [`valhalla::baldr::GraphId`] to simplify Rust bindings
using TileId = uint64_t;

enum class GraphLevel : uint8_t {
  Highway = 0,
  Arterial = 1,
  Local = 2,
};

struct TrafficEdge;

/// Exposed internal [`valhalla::baldr::GraphReader::tile_extract_t`], used to
/// access exact graph and traffic tiles. Create it using [`new_tileset()`].
struct TileSet {
  /// Explicitly define destructor as otherwise compiler will fail with
  /// std::unique_ptr due to forward declarations for `midgard::tar`
  ~TileSet();

  std::unordered_map<TileId, std::pair<char *, size_t>> tiles;
  std::unordered_map<TileId, std::pair<char *, size_t>> traffic_tiles;
  std::shared_ptr<valhalla::midgard::tar> archive;
  std::shared_ptr<valhalla::midgard::tar> traffic_archive;
  uint64_t checksum;

  rust::Vec<TileId> tiles_in_bbox(float min_lat, float min_lon, float max_lat, float max_lon, GraphLevel level) const;
  rust::Vec<TrafficEdge> get_tile_traffic(TileId id) const;
};

/// Creates a new [`TileSet`] instance based on a Valhalla's config json file
std::shared_ptr<TileSet> new_tileset(const std::string & config_file);
