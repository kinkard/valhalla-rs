#include "libvalhalla.hpp"
#include "valhalla/src/lib.rs.h"

#include <valhalla/baldr/graphreader.h>
#include <valhalla/baldr/rapidjson_utils.h>
#include <valhalla/midgard/encoded.h>

namespace baldr = valhalla::baldr;
namespace midgard = valhalla::midgard;

namespace {

struct GraphMemory : public baldr::GraphMemory {
  GraphMemory(std::pair<char*, size_t> position) {
    data = position.first;
    size = position.second;
  }
};

}  // namespace

TileSet::~TileSet() {}

std::shared_ptr<TileSet> new_tileset(const std::string& config) {
  // Hack to expose protected `baldr::GraphReader::tile_extract_t`
  struct TileSetReader : public baldr::GraphReader {
    static TileSet create(const boost::property_tree::ptree& pt) {
      auto extract = baldr::GraphReader::tile_extract_t(pt);
      return TileSet{
        .tiles = std::move(extract.tiles),
        .traffic_tiles = std::move(extract.traffic_tiles),
        .archive = std::move(extract.archive),
        .traffic_archive = std::move(extract.traffic_archive),
        .checksum = extract.checksum,
      };
    }
  };

  // `valhalla::config` uses singleton to load config only once which is not suitable for this library
  boost::property_tree::ptree pt;
  if (config.find('{') != std::string::npos) {  // `{` is illegal in file names on most systems
    auto inline_config = std::stringstream(config);
    rapidjson::read_json(inline_config, pt);
  } else {
    rapidjson::read_json(config, pt);
  }

  auto tile_set = TileSetReader::create(pt.get_child("mjolnir"));
  if (!tile_set.archive) {
    throw std::runtime_error("Failed to load tile extract from");
  }
  return std::make_shared<TileSet>(std::move(tile_set));
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
    if (tiles.find(graph_id.Tile_Base()) != tiles.end()) {
      result.push_back(graph_id);
    }
  }
  return result;
}

/// Part of the [`baldr::GraphReader::GetGraphTile()`] that gets tile from mmap file
baldr::graph_tile_ptr TileSet::get_tile(baldr::GraphId id) const {
  auto base = id.Tile_Base();

  auto tile_it = tiles.find(base);
  if (tile_it == tiles.end()) {
    return nullptr;
  }

  // Optionally get the traffic tile if it exists
  auto traffic_it = traffic_tiles.find(base);
  auto traffic = traffic_it != traffic_tiles.end() ? std::make_unique<GraphMemory>(traffic_it->second) : nullptr;

  // This initializes the tile from mmap
  return baldr::GraphTile::Create(base, std::make_unique<GraphMemory>(tile_it->second), std::move(traffic));
}

DirectedEdgeSlice directededges(const GraphTile& tile) {
  return DirectedEdgeSlice{
    .ptr = tile.directededge(0),
    .len = tile.header()->directededgecount(),
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

rust::Vec<TrafficEdge> get_tile_traffic_flows(const GraphTile& tile) {
  const auto& traffic_tile = tile.get_traffic_tile();
  if (!traffic_tile()) {
    return {};
  }

  rust::Vec<TrafficEdge> flows;
  flows.reserve(traffic_tile.header->directed_edge_count);
  for (uint32_t i = 0; i < traffic_tile.header->directed_edge_count; ++i) {
    const volatile auto& live_speed = traffic_tile.speeds[i];
    if (live_speed.speed_valid()) {
      const auto* de = tile.directededge(i);
      const auto edge_info = tile.edgeinfo(de);

      float normalized_speed = 0.0;
      if (!live_speed.closed()) {
        const uint32_t speed = tile.GetSpeed(de, baldr::kDefaultFlowMask);
        uint32_t road_speed = 0;
        for (const uint32_t speed : { edge_info.speed_limit(), de->free_flow_speed(), de->speed() }) {
          road_speed = speed;
          if (speed != 0 && speed != baldr::kUnlimitedSpeedLimit) {
            break;
          }
        }
        normalized_speed = static_cast<float>(speed) / road_speed;
      }

      flows.push_back(TrafficEdge{
          .shape = midgard::encode(edge_info.shape()),
          .normalized_speed = normalized_speed,
      });
    }
  }
  return flows;
}
