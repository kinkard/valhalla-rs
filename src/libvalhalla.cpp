#include "libvalhalla.hpp"
#include "valhalla/src/lib.rs.h"

#include <valhalla/baldr/datetime.h>
#include <valhalla/baldr/graphreader.h>
#include <valhalla/midgard/encoded.h>

#include <boost/property_tree/ptree.hpp>

namespace baldr = valhalla::baldr;
namespace midgard = valhalla::midgard;

namespace {

struct GraphMemory : public baldr::GraphMemory {
  const std::shared_ptr<midgard::tar> tar_;

  GraphMemory(std::shared_ptr<midgard::tar> tar, std::pair<char*, size_t> position) : tar_(std::move(tar)) {
    data = position.first;
    size = position.second;
  }
};

}  // namespace

TileSet::~TileSet() {}

std::shared_ptr<TileSet> new_tileset(const boost::property_tree::ptree& pt) {
  // Hack to expose protected `baldr::GraphReader::tile_extract_t`
  struct TileSetReader : public baldr::GraphReader {
    static TileSet create(const boost::property_tree::ptree& pt) {
      auto extract = baldr::GraphReader::tile_extract_t(pt, false);
      return TileSet{
        .tiles_ = std::move(extract.tiles),
        .traffic_tiles_ = std::move(extract.traffic_tiles),
        .tar_ = std::move(extract.archive),
        .traffic_tar_ = std::move(extract.traffic_archive),
      };
    }
  };

  auto tile_set = TileSetReader::create(pt.get_child("mjolnir"));
  if (!tile_set.tar_) {
    throw std::runtime_error("Failed to load tile extract");
  }
  return std::make_shared<TileSet>(std::move(tile_set));
}

rust::Vec<baldr::GraphId> TileSet::tiles() const {
  rust::vec<baldr::GraphId> result;
  result.reserve(tiles_.size());
  for (const auto& tile : tiles_) {
    result.push_back(baldr::GraphId(tile.first));
  }
  return result;
}

rust::vec<baldr::GraphId> TileSet::tiles_in_bbox(float min_lat, float min_lon, float max_lat, float max_lon,
                                                 GraphLevel level) const {
  const midgard::AABB2<midgard::PointLL> bbox(min_lon, min_lat, max_lon, max_lat);
  const auto tile_ids = baldr::TileHierarchy::levels()[static_cast<size_t>(level)].tiles.TileList(bbox);

  rust::vec<baldr::GraphId> result;
  result.reserve(tile_ids.size());
  for (auto tile_id : tile_ids) {
    const baldr::GraphId graph_id(tile_id, static_cast<uint32_t>(level), 0);
    // List only tiles that we have
    if (tiles_.find(graph_id.Tile_Base()) != tiles_.end()) {
      result.push_back(graph_id);
    }
  }
  return result;
}

/// Part of the [`baldr::GraphReader::GetGraphTile()`] that gets tile from mmap file
baldr::graph_tile_ptr TileSet::get_graph_tile(baldr::GraphId id) const {
  auto base = id.Tile_Base();

  auto tile_it = tiles_.find(base);
  if (tile_it == tiles_.end()) {
    return nullptr;
  }

  // Optionally get the traffic tile if it exists
  auto traffic_it = traffic_tiles_.find(base);
  auto traffic =
      traffic_it != traffic_tiles_.end() ? std::make_unique<GraphMemory>(traffic_tar_, traffic_it->second) : nullptr;

  // This initializes the tile from mmap
  return baldr::GraphTile::Create(base, std::make_unique<GraphMemory>(tar_, tile_it->second), std::move(traffic));
}

TrafficTile TileSet::get_traffic_tile(valhalla::baldr::GraphId id) const {
  auto base = id.Tile_Base();
  auto traffic_it = traffic_tiles_.find(base);
  if (traffic_it == traffic_tiles_.end()) {
    throw std::runtime_error("No traffic tile for the given id");
  }
  return TrafficTile(traffic_tar_, traffic_it->second);
}

uint64_t TileSet::dataset_id() const {
  if (auto it = tiles_.begin(); it != tiles_.end()) {
    return get_graph_tile(baldr::GraphId(it->first))->header()->dataset_id();
  } else {
    return 0;
  }
}

DirectedEdgeSlice directededges(const GraphTile& tile) {
  const uint32_t count = tile.header()->directededgecount();
  return DirectedEdgeSlice{
    .ptr = count ? tile.directededge(0) : nullptr,
    .len = count,
  };
}

NodeInfoSlice nodes(const GraphTile& tile) {
  const uint32_t count = tile.header()->nodecount();
  return NodeInfoSlice{
    .ptr = count ? tile.node(0) : nullptr,
    .len = count,
  };
}

NodeTransitionSlice transitions(const GraphTile& tile) {
  const uint32_t count = tile.header()->transitioncount();
  return NodeTransitionSlice{
    .ptr = count ? tile.transition(0) : nullptr,
    .len = count,
  };
}

EdgeInfo edgeinfo(const GraphTile& tile, const valhalla::baldr::DirectedEdge& de) {
  const auto edge_info = tile.edgeinfo(&de);

  rust::string shape;
  if (de.forward()) {
    // todo: use `edge_info.lazy_shape()` for better performance
    shape = midgard::encode(edge_info.shape());
  } else {
    // If the edge is not forward, we need to reverse the shape
    std::vector<valhalla::midgard::PointLL> edge_shape = edge_info.shape();
    std::reverse(edge_shape.begin(), edge_shape.end());
    shape = midgard::encode(edge_shape);
  }

  return EdgeInfo{
    .way_id = edge_info.wayid(),
    // todo: properly handle `0` and `baldr::kUnlimitedSpeedLimit`
    .speed_limit = static_cast<uint8_t>(edge_info.speed_limit()),
    // todo: directionality!
    // todo: use `edge_info.lazy_shape()` for better performance
    .shape = std::move(shape),
  };
}

uint8_t live_speed(const GraphTile& tile, const valhalla::baldr::DirectedEdge& de) {
  const volatile auto& live_speed_data = tile.trafficspeed(&de);
  if (!live_speed_data.speed_valid()) {
    return 255;  // No valid live speed data
  }
  if (live_speed_data.closed()) {
    return 0;  // Edge is closed
  }
  return live_speed_data.get_overall_speed();
}

AdminInfo admininfo(const GraphTile& tile, uint32_t index) {
  auto info = tile.admininfo(index);
  return AdminInfo{
    .country_text = info.country_text(),
    .state_text = info.state_text(),
    .country_iso = info.country_iso(),
    .state_iso = info.state_iso(),
  };
}

TimeZoneInfo from_id(uint32_t id, uint64_t unix_timestamp) {
  const date::time_zone* tz = valhalla::baldr::DateTime::get_tz_db().from_index(id);
  if (!tz) {
    throw std::runtime_error("Invalid time zone id: " + std::to_string(id));
  }

  // Because of DST, offset can change during some time of the year
  std::chrono::seconds dur(unix_timestamp);
  std::chrono::time_point<std::chrono::system_clock> tp(dur);
  const auto zoned_tp = date::make_zoned(tz, tp);
  const auto tz_info = zoned_tp.get_info();

  return TimeZoneInfo{
    .name = tz->name(),
    .offset_seconds = static_cast<int32_t>(tz_info.offset.count()),
  };
}
