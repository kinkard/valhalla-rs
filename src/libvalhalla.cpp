#include "libvalhalla.hpp"
#include "valhalla/src/lib.rs.h"

#include <valhalla/baldr/datetime.h>
#include <valhalla/baldr/graphreader.h>
#include <valhalla/midgard/encoded.h>
#include <valhalla/midgard/sequence.h>

#include <filesystem>

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

namespace {

/// Graph memory backed by a single memory-mapped tile file.
struct MappedFile : public baldr::GraphMemory {
  midgard::mem_map<char> map_;

  MappedFile(const std::string& path, size_t bytes) : map_(path, bytes, POSIX_MADV_NORMAL, true) {
    data = map_.get();
    size = bytes;
  }
};

/// Maps `dir/<tile suffix>`, or returns null when it is not a readable file.
std::unique_ptr<const baldr::GraphMemory> map_tile_file(rust::Str dir, baldr::GraphId id) {
  std::filesystem::path path{std::string(dir)};
  path /= baldr::GraphTile::FileSuffix(id.tile_base());

  std::error_code ec;
  auto bytes = std::filesystem::file_size(path, ec);
  if (ec || bytes == 0) {
    return nullptr;
  }
  try {
    return std::make_unique<MappedFile>(path.string(), bytes);
  } catch (const std::exception&) {
    return nullptr;
  }
}

/// `tile_extract_t` is protected inside `baldr::GraphReader`, same trick as `new_tileset()`.
struct ExtractPeek : public baldr::GraphReader {
  static baldr::GraphReader::tile_extract_t open(const boost::property_tree::ptree& pt, bool readonly) {
    return baldr::GraphReader::tile_extract_t(pt, readonly);
  }
};

std::unique_ptr<const baldr::GraphMemory> traffic_memory_for(const TileArchive& traffic, uint64_t base) {
  auto it = traffic.index_.find(base);
  return it != traffic.index_.end() ? std::make_unique<GraphMemory>(traffic.tar_, it->second) : nullptr;
}

}  // namespace

TileArchive::~TileArchive() {}

std::shared_ptr<TileArchive> open_graph_archive(rust::Str path) {
  boost::property_tree::ptree pt;
  pt.put("tile_extract", std::string(path));
  auto extract = ExtractPeek::open(pt, true);
  if (!extract.archive) {
    throw std::runtime_error("Failed to load tile extract");
  }
  return std::make_shared<TileArchive>(TileArchive{
    .index_ = std::move(extract.tiles),
    .tar_ = std::move(extract.archive),
  });
}

std::shared_ptr<TileArchive> open_traffic_archive(rust::Str path, bool readonly) {
  boost::property_tree::ptree pt;
  pt.put("traffic_extract", std::string(path));
  auto extract = ExtractPeek::open(pt, readonly);
  if (!extract.traffic_archive) {
    throw std::runtime_error("Failed to load traffic extract");
  }
  return std::make_shared<TileArchive>(TileArchive{
    .index_ = std::move(extract.traffic_tiles),
    .tar_ = std::move(extract.traffic_archive),
  });
}

rust::Vec<baldr::GraphId> TileArchive::tiles() const {
  rust::Vec<baldr::GraphId> result;
  result.reserve(index_.size());
  for (const auto& tile : index_) {
    result.push_back(baldr::GraphId(tile.first));
  }
  return result;
}

rust::Vec<baldr::GraphId> tile_ids_in_bbox(float min_lat, float min_lon, float max_lat, float max_lon,
                                           GraphLevel level) {
  const midgard::AABB2<midgard::PointLL> bbox(min_lon, min_lat, max_lon, max_lat);
  const auto tile_ids = baldr::TileHierarchy::levels()[static_cast<size_t>(level)].tiles.TileList(bbox);

  rust::Vec<baldr::GraphId> result;
  result.reserve(tile_ids.size());
  for (auto tile_id : tile_ids) {
    result.push_back(baldr::GraphId(tile_id, static_cast<uint32_t>(level), 0));
  }
  return result;
}

rust::Vec<baldr::GraphId> TileArchive::tiles_in_bbox(float min_lat, float min_lon, float max_lat, float max_lon,
                                                     GraphLevel level) const {
  rust::Vec<baldr::GraphId> result;
  for (auto id : tile_ids_in_bbox(min_lat, min_lon, max_lat, max_lon, level)) {
    if (index_.find(id.tile_base()) != index_.end()) {
      result.push_back(id);
    }
  }
  return result;
}

uint64_t TileArchive::dataset_id() const {
  if (auto it = index_.begin(); it != index_.end()) {
    auto tile = graph_tile(baldr::GraphId(it->first));
    auto id = tile->header()->dataset_id();
    release(tile);
    return id;
  }
  return 0;
}

const baldr::GraphTile* TileArchive::graph_tile(baldr::GraphId id) const {
  auto base = id.tile_base();
  auto it = index_.find(base);
  if (it == index_.end()) {
    return nullptr;
  }
  return baldr::GraphTile::Create(base, std::make_unique<GraphMemory>(tar_, it->second)).detach();
}

const baldr::GraphTile* TileArchive::graph_tile_with_traffic(baldr::GraphId id, const TileArchive& traffic) const {
  auto base = id.tile_base();
  auto it = index_.find(base);
  if (it == index_.end()) {
    return nullptr;
  }
  return baldr::GraphTile::Create(base, std::make_unique<GraphMemory>(tar_, it->second),
                                  traffic_memory_for(traffic, base))
      .detach();
}

TrafficTile TileArchive::traffic_tile(baldr::GraphId id) const {
  auto base = id.tile_base();
  auto it = index_.find(base);
  if (it == index_.end()) {
    throw std::runtime_error("No traffic tile for the given id");
  }

  auto header = reinterpret_cast<volatile baldr::TrafficTileHeader*>(it->second.first);
  if (header->traffic_tile_version != baldr::TRAFFIC_TILE_VERSION) {
    throw std::runtime_error("Unsupported TrafficTile version");
  }
  if (sizeof(baldr::TrafficTileHeader) + header->directed_edge_count * sizeof(baldr::TrafficSpeed) !=
      it->second.second) {
    throw std::runtime_error("TrafficTile data size does not match header count");
  }

  return TrafficTile{
    .header = reinterpret_cast<uint64_t*>(it->second.first),
    .speeds = reinterpret_cast<uint64_t*>(it->second.first + sizeof(baldr::TrafficTileHeader)),
    .edge_count = header->directed_edge_count,
    .traffic_tar = tar_,
  };
}

const baldr::GraphTile* graph_tile_from_dir(rust::Str dir, baldr::GraphId id) {
  return baldr::GraphTile::Create(std::string(dir), id.tile_base()).detach();
}

const baldr::GraphTile* graph_tile_from_dir_with_traffic(rust::Str dir, baldr::GraphId id,
                                                         const TileArchive& traffic) {
  auto base = id.tile_base();
  return baldr::GraphTile::Create(std::string(dir), base, traffic_memory_for(traffic, base)).detach();
}

const baldr::GraphTile* graph_tile_from_dir_mmap(rust::Str dir, baldr::GraphId id) {
  auto memory = map_tile_file(dir, id);
  if (!memory) {
    return nullptr;
  }
  return baldr::GraphTile::Create(id.tile_base(), std::move(memory)).detach();
}

const baldr::GraphTile* graph_tile_from_dir_mmap_with_traffic(rust::Str dir, baldr::GraphId id,
                                                              const TileArchive& traffic) {
  auto base = id.tile_base();
  auto memory = map_tile_file(dir, id);
  if (!memory) {
    return nullptr;
  }
  return baldr::GraphTile::Create(base, std::move(memory), traffic_memory_for(traffic, base)).detach();
}

namespace {

/// Graph memory over a buffer Rust handed over. A `rust::Vec`'s heap buffer does not move when the
/// `Vec` itself does, so `data` stays valid for this object's lifetime; destroying it frees the
/// buffer through Rust's allocator.
struct RustMemory : public baldr::GraphMemory {
  rust::Vec<uint8_t> buf_;

  explicit RustMemory(rust::Vec<uint8_t> buf) : buf_(std::move(buf)) {
    data = reinterpret_cast<char*>(buf_.data());
    size = buf_.size();
  }
};

}  // namespace

// If `Initialize` throws, `RustMemory` is already owned by the half-built tile and is destroyed
// during unwinding, so the buffer is freed on the failure path too.
const baldr::GraphTile* graph_tile_from_memory(baldr::GraphId id, rust::Vec<uint8_t> bytes) {
  return baldr::GraphTile::Create(id.tile_base(), std::make_unique<RustMemory>(std::move(bytes))).detach();
}

const baldr::GraphTile* graph_tile_from_memory_with_traffic(baldr::GraphId id, rust::Vec<uint8_t> bytes,
                                                            const TileArchive& traffic) {
  auto base = id.tile_base();
  return baldr::GraphTile::Create(base, std::make_unique<RustMemory>(std::move(bytes)),
                                  traffic_memory_for(traffic, base))
      .detach();
}

rust::String tile_file_suffix(baldr::GraphId id, bool gzipped) {
  return baldr::GraphTile::FileSuffix(id.tile_base(),
                                      gzipped ? baldr::SUFFIX_COMPRESSED : baldr::SUFFIX_NON_COMPRESSED, true);
}

baldr::GraphId tile_id_from_path(rust::Str path) {
  return baldr::GraphId(baldr::GraphTile::GetTileId(std::string(path)));
}

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
    if (tiles_.find(graph_id.tile_base()) != tiles_.end()) {
      result.push_back(graph_id);
    }
  }
  return result;
}

/// Part of the [`baldr::GraphReader::GetGraphTile()`] that gets tile from mmap file
const baldr::GraphTile* TileSet::get_graph_tile(baldr::GraphId id) const {
  auto base = id.tile_base();

  auto tile_it = tiles_.find(base);
  if (tile_it == tiles_.end()) {
    return nullptr;
  }

  // Optionally get the traffic tile if it exists
  auto traffic_it = traffic_tiles_.find(base);
  auto traffic =
      traffic_it != traffic_tiles_.end() ? std::make_unique<GraphMemory>(traffic_tar_, traffic_it->second) : nullptr;

  // cxx doesn't support `boost::intrusive_ptr<T>`, so instead all refcounting should be done manually
  auto ptr = baldr::GraphTile::Create(base, std::make_unique<GraphMemory>(tar_, tile_it->second), std::move(traffic));
  return ptr.detach();
}

TrafficTile TileSet::get_traffic_tile(baldr::GraphId id) const {
  auto base = id.tile_base();
  auto traffic_it = traffic_tiles_.find(base);
  if (traffic_it == traffic_tiles_.end()) {
    throw std::runtime_error("No traffic tile for the given id");
  }

  auto header = reinterpret_cast<volatile baldr::TrafficTileHeader*>(traffic_it->second.first);
  if (header->traffic_tile_version != baldr::TRAFFIC_TILE_VERSION) {
    throw std::runtime_error("Unsupported TrafficTile version");
  }
  if (sizeof(baldr::TrafficTileHeader) + header->directed_edge_count * sizeof(baldr::TrafficSpeed) !=
      traffic_it->second.second) {
    throw std::runtime_error("TrafficTile data size does not match header count");
  }

  return TrafficTile{
    .header = reinterpret_cast<uint64_t*>(traffic_it->second.first),
    .speeds = reinterpret_cast<uint64_t*>(traffic_it->second.first + sizeof(baldr::TrafficTileHeader)),
    .edge_count = header->directed_edge_count,
    .traffic_tar = traffic_tar_,
  };
}

uint64_t TileSet::dataset_id() const {
  if (auto it = tiles_.begin(); it != tiles_.end()) {
    return get_graph_tile(baldr::GraphId(it->first))->header()->dataset_id();
  } else {
    return 0;
  }
}

LatLon node_latlon(const baldr::GraphTile& tile, const baldr::NodeInfo& node) {
  const auto base_ll = tile.header()->base_ll();
  const auto ll = node.latlng(base_ll);
  return LatLon{ .lat = ll.lat(), .lon = ll.lng() };
}

EdgeInfo edgeinfo(const baldr::GraphTile& tile, const baldr::DirectedEdge& de) {
  // `baldr::EdgeInfo` keeps these pointers protected, and a local class cannot hold static data
  // members - hence the `using`s and the pointers-to-member as locals.
  struct EdgeInfoPeek : baldr::EdgeInfo {
    using baldr::EdgeInfo::encoded_shape_;
  };
  constexpr auto shape_ptr = &EdgeInfoPeek::encoded_shape_;

  const auto edge_info = tile.edgeinfo(&de);

  const auto* shape = reinterpret_cast<const uint8_t*>(edge_info.*shape_ptr);
  const uint32_t shape_size = edge_info.encoded_shape_size();

  return EdgeInfo{
    .way_id = edge_info.wayid(),
    // todo: properly handle `0` and `baldr::kUnlimitedSpeedLimit`
    .speed_limit = static_cast<uint8_t>(edge_info.speed_limit()),
    .encoded_shape = rust::Slice<const uint8_t>(shape, shape_size),
  };
}

uint64_t live_traffic(const baldr::GraphTile& tile, const baldr::DirectedEdge& de) {
  static_assert(sizeof(baldr::TrafficSpeed) == sizeof(uint64_t), "TrafficSpeed must be a single u64");
  // trafficspeed() throws if `de` is out of tile bounds - guarded by the Rust-side `debug_assert!`
  // ("Wrong tile") and load-time tile validation.
  const volatile baldr::TrafficSpeed& ts = tile.trafficspeed(&de);
  return *reinterpret_cast<const volatile uint64_t*>(&ts);
}

AdminInfo admininfo(const baldr::GraphTile& tile, uint32_t index) {
  auto info = tile.admininfo(index);
  return AdminInfo{
    .country_text = info.country_text(),
    .state_text = info.state_text(),
    .country_iso = info.country_iso(),
    .state_iso = info.state_iso(),
  };
}

TimeZoneInfo from_id(uint32_t id, uint64_t unix_timestamp) {
  const date::time_zone* tz = baldr::DateTime::get_tz_db().from_index(id);
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

inline volatile baldr::TrafficTileHeader* tile_header(const TrafficTile& tile) {
  return reinterpret_cast<volatile baldr::TrafficTileHeader*>(const_cast<uint64_t*>(tile.header));
}

baldr::GraphId id(const TrafficTile& tile) {
  auto header = tile_header(tile);
  return baldr::GraphId(header->tile_id);
}

uint64_t last_update(const TrafficTile& tile) {
  auto header = tile_header(tile);
  return header->last_update;
}

void write_last_update(const TrafficTile& tile, uint64_t t) {
  auto header = tile_header(tile);
  header->last_update = t;
}

uint64_t spare(const TrafficTile& tile) {
  auto header = tile_header(tile);
  return (static_cast<uint64_t>(header->spare2) << 32) | header->spare3;
}

void write_spare(const TrafficTile& tile, uint64_t s) {
  auto header = tile_header(tile);
  header->spare2 = static_cast<uint32_t>(s >> 32);
  header->spare3 = static_cast<uint32_t>(s & 0xFFFFFFFF);
}
