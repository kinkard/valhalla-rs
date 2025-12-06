#pragma once

#include <valhalla/baldr/graphtile.h>
#include <boost/property_tree/ptree_fwd.hpp>
#include <cstdint>

#include "cxx.h"

namespace valhalla::midgard {
struct tar;
}

struct AdminInfo;
struct EdgeInfo;
struct TimeZoneInfo;
struct TrafficTile;
struct LatLon;

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

  rust::Vec<valhalla::baldr::GraphId> tiles() const;
  rust::Vec<valhalla::baldr::GraphId> tiles_in_bbox(float min_lat, float min_lon, float max_lat, float max_lon,
                                                    GraphLevel level) const;
  valhalla::baldr::graph_tile_ptr get_graph_tile(valhalla::baldr::GraphId id) const;
  TrafficTile get_traffic_tile(valhalla::baldr::GraphId id) const;
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
inline rust::Slice<const valhalla::baldr::DirectedEdge> directededges(const GraphTile& tile) {
  auto slice = tile.GetDirectedEdges();
  return rust::Slice(slice.data(), slice.size());
}

/// Helper function that allows to iterate over a slice of nodes of that tile in Rust
inline rust::Slice<const valhalla::baldr::NodeInfo> nodes(const GraphTile& tile) {
  auto slice = tile.GetNodes();
  return rust::Slice(slice.data(), slice.size());
}

/// Helper function that allows to iterate over a slice of node transitions of that tile in Rust
inline rust::Slice<const valhalla::baldr::NodeTransition> transitions(const GraphTile& tile) {
  // apparently, `tile.GetNodeTransitions()` requires `NodeInfo*` to return only transitions for that node.
  const uint32_t count = tile.header()->transitioncount();
  return rust::Slice(count ? tile.transition(0) : nullptr, count);
}

/// Helper function that allows to iterate over a slice of node edges of that tile in Rust
inline rust::Slice<const valhalla::baldr::DirectedEdge> node_edges(const GraphTile& tile,
                                                                   const valhalla::baldr::NodeInfo& node) {
  auto edges = tile.GetDirectedEdges();
  // Safety: Rust side of bindings has an assert that this node belongs to the given tile.
  return rust::Slice(edges.data() + node.edge_index(), node.edge_count());
}

/// Helper function that allows to iterate over a slice of node transitions of that tile in Rust
inline rust::Slice<const valhalla::baldr::NodeTransition> node_transitions(const GraphTile& tile,
                                                                           const valhalla::baldr::NodeInfo& node) {
  auto slice = tile.GetNodeTransitions(&node);
  return rust::Slice(slice.data(), slice.size());
}

/// Helper function to get lat,lng for the given node
LatLon node_latlon(const GraphTile& tile, const valhalla::baldr::NodeInfo& node);

/// Helper function that workarounds the inability to use `baldr::EdgeInfo` in Rust
EdgeInfo edgeinfo(const GraphTile& tile, const valhalla::baldr::DirectedEdge& de);

/// Helper method that returns 0 if the edge is closed, 255 if live speed in unknown and speed in km/h otherwise
uint8_t live_speed(const GraphTile& tile, const valhalla::baldr::DirectedEdge& de);

/// Helper function to get admin info for a given index
AdminInfo admininfo(const GraphTile& tile, uint32_t index);

/// Helper function to resolve tz name and offset from a given id and unix timestamp.
TimeZoneInfo from_id(uint32_t id, uint64_t unix_timestamp);

struct TrafficTile {
  volatile valhalla::baldr::TrafficTileHeader* header;
  // Rust doesn't support bitfields, so we expose this as a raw u64 pointer.
  // The underlying data is an array of `valhalla::baldr::TrafficSpeed` structs,
  // each of which is exactly 64 bits.
  volatile uint64_t* speeds;
  std::shared_ptr<valhalla::midgard::tar> traffic_tar;

  TrafficTile(std::shared_ptr<valhalla::midgard::tar> tar, std::pair<char*, size_t> position) : traffic_tar(tar) {
    using namespace valhalla::baldr;

    header = reinterpret_cast<volatile TrafficTileHeader*>(position.first);
    speeds = reinterpret_cast<volatile uint64_t*>(position.first + sizeof(TrafficTileHeader));

    if (header->traffic_tile_version != TRAFFIC_TILE_VERSION) {
      throw std::runtime_error("Unsupported TrafficTile version");
    }
    if (sizeof(TrafficTileHeader) + header->directed_edge_count * sizeof(TrafficSpeed) != position.second) {
      throw std::runtime_error("TrafficTile data size does not match header count");
    }
  }

  valhalla::baldr::GraphId id() const { return valhalla::baldr::GraphId(header->tile_id); }

  uint64_t last_update() const { return header->last_update; }
  void write_last_update(uint64_t t) const { header->last_update = t; }

  uint64_t spare() const { return (static_cast<uint64_t>(header->spare2) << 32) | header->spare3; }
  void write_spare(uint64_t s) const {
    header->spare2 = static_cast<uint32_t>(s >> 32);
    header->spare3 = static_cast<uint32_t>(s & 0xFFFFFFFF);
  }

  uint32_t edge_count() const { return header->directed_edge_count; }

  void clear_traffic() const {
    const uint32_t count = header->directed_edge_count;
    for (uint32_t i = 0; i < count; ++i) {
      speeds[i] = 0;
    }
    header->last_update = 0;
  }
};

/// Helper function to get traffic data for a given edge index
inline uint64_t edge_traffic(const TrafficTile& tile, uint32_t edge_index) {
  if (edge_index < tile.header->directed_edge_count) {
    return tile.speeds[edge_index];
  }
  throw std::runtime_error(
      "TrafficSpeed requested for edgeid beyond bounds of tile (offset: " + std::to_string(edge_index) +
      ", edge count: " + std::to_string(tile.header->directed_edge_count));
}

/// Helper function to write traffic data for a given edge index
inline void write_edge_traffic(const TrafficTile& tile, uint32_t edge_index, uint64_t traffic) {
  if (edge_index < tile.header->directed_edge_count) {
    tile.speeds[edge_index] = traffic;
    return;
  }
  throw std::runtime_error(
      "TrafficSpeed requested for edgeid beyond bounds of tile (offset: " + std::to_string(edge_index) +
      ", edge count: " + std::to_string(tile.header->directed_edge_count));
}

/// Helper function that encodes predicted speeds from a slice of floats and returns a base64 string
inline rust::String encode_weekly_speeds(rust::Slice<const float> speeds) {
  if (speeds.size() != valhalla::baldr::kBucketsPerWeek) {
    throw std::runtime_error("Weekly speeds slice size must be equal to " +
                             std::to_string(valhalla::baldr::kBucketsPerWeek));
  }

  auto compressed = valhalla::baldr::compress_speed_buckets(speeds.data());
  return valhalla::baldr::encode_compressed_speeds(compressed.data());
}

/// Helper function that decodes predicted speeds from a base64 string into an array or floats
inline rust::Vec<float> decode_weekly_speeds(rust::Str encoded) {
  // todo: replace by std::string_view once Valhalla supports it
  std::string encoded_str(encoded.data(), encoded.size());
  const auto coefficients = valhalla::baldr::decode_compressed_speeds(encoded_str);

  rust::Vec<float> speeds;
  speeds.reserve(valhalla::baldr::kBucketsPerWeek);
  for (uint32_t i = 0; i < valhalla::baldr::kBucketsPerWeek; ++i) {
    float speed = valhalla::baldr::decompress_speed_bucket(coefficients.data(), i);
    speeds.push_back(speed);
  }

  return speeds;
}
