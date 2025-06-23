#include "libvalhalla.hpp"
#include "libvalhalla/src/lib.rs.h"

#include <valhalla/baldr/graphreader.h>
#include <valhalla/config.h>
#include <valhalla/midgard/encoded.h>

namespace baldr = valhalla::baldr;
namespace midgard = valhalla::midgard;

namespace {

struct GraphMemory : public baldr::GraphMemory {
  GraphMemory(std::pair<char *, size_t> position) {
    data = position.first;
    size = position.second;
  }
};

}  // namespace

TileSet::~TileSet() {}

std::shared_ptr<TileSet> new_tileset(const std::string & config_file) {
  // Hack to expose protected `baldr::GraphReader::tile_extract_t`
  struct TileSetReader : public baldr::GraphReader {
    static TileSet create(const boost::property_tree::ptree & pt) {
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

  boost::property_tree::ptree config = valhalla::config(config_file);
  return std::make_shared<TileSet>(TileSetReader::create(config.get_child("mjolnir")));
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

rust::Vec<TrafficEdge> get_tile_traffic_flows(const GraphTile & tile) {
  const auto & traffic_tile = tile.get_traffic_tile();
  if (!traffic_tile()) {
    return {};
  }

  rust::Vec<TrafficEdge> flows;
  flows.reserve(traffic_tile.header->directed_edge_count);
  for (uint32_t i = 0; i < traffic_tile.header->directed_edge_count; ++i) {
    const volatile auto & live_speed = traffic_tile.speeds[i];
    if (live_speed.speed_valid()) {
      const auto * de = tile.directededge(i);
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
