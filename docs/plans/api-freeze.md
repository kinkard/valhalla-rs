# Rust API freeze

Merging `valhalla-rs` into Valhalla means adopting Valhalla's versioning, and Valhalla upstream is
stable enough that the API we ship at the merge is the API we keep. This is the one chance to break
things, so everything worth breaking goes in a single release.

Status: draft, iterating.

## Principles

Settled, and everything below is judged against them:

1. **Mirror the C++ API.** Users arriving from Valhalla's C++ should not have to learn a second set
   of names. Transparency beats Rust idiom when the two conflict.
2. **The most performant way is the easiest one.** The proto API is the fast path, so it is the
   path with no friction. Making the JSON path slightly less convenient is a feature.
3. **No cost the caller did not ask for.** Nothing decodes, allocates, or synchronises unless the
   caller asked for that specific thing.
4. **Tiles stay dependency-light.** Reading tiles must not require protobuf - see `traffic_debug`.

## Baseline

Andorra tileset, 7 tiles / 30,418 directed edges / 13,153 nodes, `cargo bench --bench api_bench`
(`benches/api_bench.rs`, scratch - delete once the work lands). Bench profile has `lto = "thin"`,
which is worth 20-30% on FFI-heavy loops on its own - see
[cross-language LTO](#cross-language-lto-is-not-actually-on).

| | today | achievable | |
|---|---|---|---|
| `edgeinfo(de).way_id` per edge | 290 ns | 4.8 ns | 60× |
| all shape points, whole tileset | 8.77 ms + caller-side polyline decode | 1.23 ms | >7× |
| tile per node id, naive traversal | 67 ns | 10 ns | 6× (caller's cache) |
| `tile.directededges()` called in a loop | 31 µs | 4.5 µs hoisted | 7× (idiom, not API) |
| `GraphId::from_parts` | 1.91 ns | 0.70 ns | 3× |
| always-on tile bounds check | 24.76 µs | 24.71 µs | free |

The two `edgeinfo` rows predate elevation joining the view, which costs ~1 ns/edge - re-measure
before quoting them.

## P0 - performance and soundness

### 1. Lazy `EdgeInfo`

`ffi::edgeinfo()` decodes the shape into a `vector<PointLL>`, re-encodes it to polyline6 and copies
that into a `rust::String` - three allocations and a full shape round-trip - even when the caller
only wanted `way_id`. That is the 293 ns.

Replace with a borrowed view: scalars are field reads, and the shape stays in the tile until asked
for. Shape7 is zigzag varints at 1e-6 - an encoded polyline without the ASCII offset - so Rust
decodes it directly from the mmap with no allocation.

- [x] `GraphTile::edgeinfo()` returns a borrowed `EdgeInfo<'_>`, the eager shared struct is gone
- [x] shape as an iterator over the tile bytes; `EdgeInfo::encoded_shape() -> &[u8]` for raw bytes
- [x] elevation as an iterator - see [Elevation](#elevation)
- [x] `DirectedEdge::forward()` exposed. It was never bound, which is the only reason the prototype
      copied it into the view - a per-edge fact on a type both edges of a pair share
- [x] `NodeInfo::elevation()` returns `Option<f32>` instead of a `-500.0` sentinel, matching
      `EdgeInfo::mean_elevation()`. Needed the moment anything consumes elevation
- [ ] tagged values - deferred, see [Tag values](#tag-values)
- [x] `encoded_shape_` and `encoded_elevation_` are both `protected`, so borrowing those bytes has
      to defeat access control. Naming them through a derived class as pointers-to-member gives
      *base*-relative pointers, defined behaviour on any `baldr::EdgeInfo` - unlike the prototype's
      derived-class cast, which downcast an object that never was one, and was UB. Measured free:
      `way_id` over the tileset 110.49 µs against the cast's 109.42 µs in the same window, well
      inside the ~6% this box drifts by between runs.
- [ ] **upstream still wants** public accessors - they delete the shim, and it is the same PR as the
      tags iterator, see [Tag values](#tag-values)

Open, see [Shape direction](#shape-direction) and [Shape type](#shape-type).

### 2. Tile-membership checks

`node_edges`, `edgeinfo`, `live_traffic` and `node_latlon` only `debug_assert!` that the reference
belongs to the tile. In release, a reference from another tile reads out of bounds from a safe
function.

Do the check inside the C++ helper, where the node array is already at hand - no extra FFI hop, and
no need for the Rust wrapper to hold anything. Measured over every node in the tileset:
unchecked 28.16 µs, checked C++-side 28.10 µs. Free.

- [ ] validate in the C++ helpers for all four
- [ ] decide the failure mode: empty slice, panic, or `Result`

## P1 - types

### 3. `TileId`

`tiles()` returns `Vec<GraphId>` whose `id()` is always 0, and `graph_tile()` silently ignores the
id bits of whatever it is handed. A distinct type says which of the three things an id is.

Measured, since the padding question was open: `HashMap<TileId, GraphTile>` is **16 bytes per entry
either way** - `(u32, ptr)` pads back up. Real savings are `Vec<TileId>` (1.6 MB → 0.8 MB at planet
scale), `HashSet<TileId>` (8 → 4 bytes), `HashMap<TileId, u32>` (16 → 8). So the case is type
safety, with memory as a bonus in the cases that happen to pack.

- [ ] `TileId`; `GraphId::tile() -> TileId`, `graph_tile(TileId)`, `tiles() -> Vec<TileId>`
- [ ] decide whether `NodeId` / `EdgeId` follow - see [Id newtypes](#id-newtypes)

### 4. `GraphLevel` everywhere

`GraphId::level()` returns `u32` while `tiles_in_bbox()` takes `GraphLevel`. Verified that cxx
shared enums are already open - they compile to a `#[repr(transparent)]` struct with associated
constants, so an out-of-range repr is representable and `match` needs a catch-all. No `Option`, no
lossy conversion.

- [ ] `GraphId::level() -> GraphLevel`

### 5. One `LatLon`

Two types share the name today (`ffi::LatLon` with fields, `crate::LatLon` as a tuple). Positional
`LatLon(f64, f64)` next to a C++ `PointLL` that is `(lon, lat)` is a swap waiting to happen.

- [ ] unify into one type; decide named fields vs accessors on the tuple

### 6. Remove deprecated

- [ ] all 9, `CostingModel` included

## P2 - ergonomics

- [ ] `tile.edges()` / `tile.nodes()` yielding `(GraphId, &T)`, plus `tile.edge_id(de)` - every
      example does index arithmetic to recover an id it already had. `slice::element_offset` makes
      this safe and trivial, and is what would earn the MSRV bump - see [MSRV](#msrv)
- [ ] `Error` as an enum with a `Cxx(String)` fallback; stop dropping the reason in
      `traffic_tile()` and `admin_info()` via `.ok()`
- [ ] `edge_speed(de, sources, is_truck, second_of_week, seconds_from_now)` - five positional args,
      two of them unrelated time quantities. A query struct
- [ ] pure-Rust `GraphId::from_parts` - it is bit math behind an FFI hop and a `Result` (4×)

## Rejected

Recorded so they do not come back.

**Idiomatic Rust naming.** Keeping `directededges`, `forwardaccess`, `endnode`, `kMotorway` and
friends. A C++ Valhalla user should recognise the API; that mental model is worth more than
`snake_case` purity. (Verified in passing that `#[cxx_name]` does work on enum variants, so
`RoadClass::Motorway` → `kMotorway` was available - we are choosing not to use it.)

**Typed JSON responses / `route()` + `route_json()` split.** Valhalla's serializers are ~9,300
lines, of which the OSRM route serializer alone is 2,489 - mirroring both schemas as Rust structs
is a maintenance sink for a generic binding. The slight friction of the JSON path is desirable: it
pushes callers to proto, which is the fast path. Anyone who wants JSON already has their own
structs. `Response` stays.

**Sorting `tiles()`.** Order is `unordered_map` order today. Sorting imposes a cost on every caller
for a property most do not need. Document that the order is unspecified instead.

**`GraphTile: Send` via an atomic refcount.** Would tax every `intrusive_ptr` copy, including
Valhalla's own hot path (~10% on Actor), and cannot be changed for the bindings alone. The
per-task idiom - hand each task its own `GraphReader` clone or reference - is the right one anyway.

**Fat `GraphTile`.** Caching the edge/node/transition slices in the Rust wrapper would make
`tile.directededges()` a field read instead of a 2.4 ns FFI call, but the wrapper is a bare pointer
to the C++ object today and duplicating its state is a maintenance burden. Calling the accessor in
a loop costs 9× - that is an idiom to document (hoist the slice), not an API to change.

**A tile cache in the crate.** `graph_tile()` costs 74 ns against 2 ns to clone a cached tile, and
every traversal example hand-rolls a cache - but they can skip eviction only because their searches
are bounded by other means. A library cache needs a growth policy, and that belongs to the caller.
If it ever comes back, the answer is Valhalla's own bounded `TileCacheLRU`, not a new Rust map.

**Moving `baldr::EdgeInfo` into the access shim.** The cleanest way to reach its protected pointers
is to move the record into a derived object, which owns them outright - no cast to defend. But
`EdgeInfo` carries a `std::vector` and a `std::multimap`, so every edge pays their move plus a
second destructor, and the optimiser cannot elide either across the opaque `tile.edgeinfo()` call:
`way_id` over the tileset 183.86 µs against 109.42 µs, +2.5 ns/edge. A mirror with the same 120
bytes but trivially movable members moved for free, so it is those two members, not the size. They
are exactly what the borrowed view exists to never touch. Pointers-to-member cost nothing instead.

**Manual `ptr + len` across FFI.** Would save 0.85 ns/edge on `EdgeInfo` - the profile agrees, at
22% of the call in `cxxbridge1$slice$new` - but `directededges`, `nodes`, `node_edges` and
`transitions` would all want the same treatment. Consistency wins; fix it in cxx instead if it ever
matters.

**Dropping the `proto` feature.** Reading tiles should not drag in protobuf; `traffic_debug` is the
reference consumer.

## Open questions

### Cross-language LTO is not actually on

`build.rs` intends it (`has_lld()`, `-flto=thin`, CMake IPO) but only half of the pair is wired up:
nothing passes `-Clinker-plugin-lto` to rustc, and `has_lld()` returns `true` for any
`apple-darwin` target without checking - there is no `lld` on this machine at all. So we get
C++-internal LTO, not Rust↔C++.

Not reachable here either: cross-language LTO needs rustc and clang to agree on LLVM version, and
this box has rustc on LLVM 22.1.2 against Apple clang 17.0.0. A matching upstream clang on Linux
could work; Apple clang never will. Worth confirming on CI before counting on it.

A profile says what that costs. `scripts/profile.sh 'edgeinfo/way_id_whole_tileset'` (samply, self
time per symbol, `--profile-time` so no criterion analysis is in the samples): **22%** of the call
is `cxxbridge1$slice$new`, called twice per edge and never inlined - it is a Rust function reached
from C++, which is exactly what cross-language LTO would fix.

Rust-side ThinLTO is a different thing, and it is on for `[profile.bench]` only. It inlines the
cxx-generated Rust shims into the caller:

| | no LTO | thin LTO |
|---|---|---|
| `directededges()` FFI call | 2.39 ns | 1.87 ns |
| scalar-only FFI floor, per edge | 4.19 ns | 3.37 ns |
| `GraphId::from_parts` (FFI) | 2.79 ns | 1.91 ns |

Benching under LTO is the point: it keeps us optimising what the compiler cannot do for us instead
of chasing overhead a release build already removes.

Nothing beyond that. Cargo reads profiles only from the workspace root of the build being run, so a
library cannot enable LTO for its users - and does not need to, since a consumer whose own profile
enables it gets bitcode from every dependency automatically. Setting it in the examples was
reverted as ceremony that bought nothing.

What ThinLTO does *not* touch is `rust::Slice` construction in C++: `sliceInit` lives in `cxx.cc`,
which the `cxx` crate compiles with its own flags, and it calls `cxxbridge1$slice$new`, a Rust
symbol. Two calls neither side can inline, on every slice built in C++ - `directededges`, `nodes`,
`node_edges`, `transitions` and the `EdgeInfo` shape all pay it.

### Shape direction

Settled, and it lands differently for the two encodings - which is the point.

`shape()` yields **storage order** (the way's direction) and the caller checks
`DirectedEdge::forward()`, as Valhalla does. Edge order would mean materialising, because a varint
stream cannot be walked backwards. `ferry-lines` is the live example: following a route needs edge
order, so a reverse edge is appended in storage order and that range of the output is flipped in
place. The buffering is unavoidable; a `Vec` per edge is not - a forward edge extends the output
straight from the iterator, and a reverse one reuses the space it just wrote.

`elevation()` yields **travel order**, because fixed-size `i8` deltas *can* be walked backwards -
see [Elevation](#elevation).

So the inconsistency is real but earned: each accessor is as good as its encoding allows, and the
docs say which order you get. The alternative - forcing shape's limitation onto elevation for
symmetry - would cost an allocation per reverse edge for nothing.

### Shape type

Mostly settled. `EdgeInfo::shape()` returns `Shape<'a>`, an allocation-free iterator over the tile
bytes, modelled on `polyline-iter`'s `BinaryPolylineIter`: bounded varint loop with a single
re-slice, `len()` / `is_empty()` / `size_hint()` / `count()` / `Clone` / `FusedIterator`. Adopting
that shape was also 10% faster than the naive per-byte version (1.37 ms → 1.23 ms).

The encoding itself cannot be shared with `polyline-iter` - its binary format packs one varint per
point and bit-splits it, while Valhalla writes two zigzag varints. And a dependency is the wrong
answer for code heading into Valhalla's tree, so ~40 lines stay vendored.

`impl From<LatLon> for (f64, f64)` keeps interop one `.map()` away:

```rust
let polyline6 = polyline_iter::encode(6, tile.edgeinfo(de).shape().map(Into::into));
```

Open: does anything else come with it - an `EncodedShape` newtype over the raw bytes, a
`to_polyline6()` convenience? Given the line above, probably not.

Settled while measuring: the shape crosses as a borrowed `&'a [u8]` in a lifetime-parameterised
shared struct (cxx generates `rust::Slice<const uint8_t>`), same as every other slice in the API.
It costs 0.85 ns/edge over a raw pointer, all of it `rust::Slice` construction - see
[cross-language LTO](#cross-language-lto-is-not-actually-on). Not worth a bespoke `ptr + len`
struct that `directededges`, `nodes` and `transitions` would then also want.

### Elevation

Format re-derived from the constructor, the encoder, the decoder and its one production caller
(`triplegbuilder.cc` `SetElevation`), after the first reading turned out to be asserted more
confidently than it had been checked:

- `int8_t` deltas at 0.25 m, pointer set last in `EdgeInfo`'s constructor - after the shape and the
  optional extended way-id bytes
- count is `encoded_elevation_count(edge->length())`, which is exactly `length / 32`: the three
  float divisions cancel. Verified for every length 1..5,000,000, zero mismatches
- the chain is anchored **only** at the storage-start node. The end node's elevation comes from
  `NodeInfo` and is *not* derivable from the deltas, which is why both ends are needed
- samples are uniform from 0 to `length` inclusive, spacing `length / (n + 1)`

API, driven by writing the consumer code rather than reasoning about it:

- `EdgeInfo::elevation(edge, start, end)` yields the samples **between** the nodes, in travel order.
  The caller has both nodes anyway - a path walk needs the far one to continue - so taking both lets
  the direction handling move inside
- `Elevation` is a `DoubleEndedIterator`. Keeping `back == front + sum(remaining)` makes stepping
  from the front leave the far end untouched, so both ends advance independently; the sum is computed
  once, lazily, so forward-only iteration never pays for it. A regression test with cancelling deltas
  guards the laziness
- the sampling interval needs no API of its own: `samples.len()` is `n`, so it is
  `length / (len() + 1)`
- elevation rides **in the view** rather than behind a lazy FFI call. In the view costs ~1 ns/edge
  for a second `rust::Slice` construction; lazy cost 4-5 ns to rebuild `baldr::EdgeInfo` for anyone
  who wanted it. Rebuilding to re-read a pointer the first call already had was the wrong shape

Two properties a consumer cannot see from the API, worth documenting when this is written up:

- one step is capped at +31.75 / -32.0 m, and the encoder **clamps and carries** the remainder into
  the next delta, so a decoded profile can trail the true one across something very steep. An error
  is only flagged past ±256 (±64 m)
- missing elevation is stored as a *zero delta*, not a gap, so NO_DATA decodes as "same as the
  previous sample" and is indistinguishable from genuinely flat ground

**Unverified against real bytes.** Andorra carries elevation on 0 of 30,418 edges, so the only
coverage is a unit test of the arithmetic against my own reading of the encoder - which agrees with
the bug if the reading is wrong. Shape is verified point-for-point against C++; elevation is not.
Closing it needs a fixture: `valhalla-s3-elevation --bbox` fetches the tiles, but this build sets
`ENABLE_DATA_TOOLS=OFF`, so `valhalla_build_tiles` has to come from a separate `../valhalla` build.

### Tag values

Tagged values live in the tile's names list: a `NameInfo` with `tagged_` set points at an entry
whose first byte is the `TaggedValue` discriminant, followed by a payload whose length rule depends
on the tag - NUL-terminated for `kLayer` / `kLevelRef` / `kTunnel` / `kBridge` / `kBssInfo`, a
9-byte header plus name for `kLandmark`, varint-length-prefixed for `kLevels` / `kOSMNodeIds`,
fixed records for `kConditionalSpeedLimits`, and `kLinguistic` handled apart from all of it.

Everything in C++ funnels through `EdgeInfo::GetTags()`, which walks the names list and builds a
`std::multimap<TaggedValue, std::string>` - one `std::string` plus one map node per tag - cached in
a `mutable` member. `layer()`, `levels()`, `level_ref()`, `osm_node_ids()`,
`conditional_speed_limits()` and `includes_level()` are all lookups on that cache.

That cache is the problem for us: it lives in the `baldr::EdgeInfo` object, and our design builds a
temporary one per FFI call. Every tag accessor would rebuild the whole multimap, so reading two
tags off one edge pays for the walk and the allocations twice.

Options:

1. **Per-tag FFI calls** mirroring the C++ accessors. Simplest, matches principle 1, but rebuilds
   the multimap per call - fine for reading one tag on a few edges, bad for a tileset scan.
2. **One call returning all tags** into a Rust-side structure. One rebuild per edge instead of per
   accessor, still allocating.
3. **Rust walks the names list** and yields `(TaggedValue, &[u8])` borrowed from the tile. Zero
   allocation, but duplicates `TaggedValueSize`'s per-tag length rules in Rust - which silently
   break the day upstream adds a tag type.
4. **Fix it upstream.** `get_tagged_value()` in `edgeinfo.cc` *already* returns a
   `std::string_view` into the tile; `GetTags()` copies it into a `std::string` only to cache it.
   A view-based iterator over the names list is a small addition that removes the allocation for
   C++ callers too, and Rust then gets `tags() -> impl Iterator<Item = (TaggedValue, &[u8])>` with
   no duplicated parsing.

Leaning 4, with the typed helpers (`levels()`, `osm_node_ids()`) delegating to C++ for the decode,
since those payloads are varint-encoded structures rather than plain bytes. Same shape as the
`encoded_shape_` accessor: the thing that makes it clean is being inside Valhalla.

The cache costs us before we read a single tag: `baldr::EdgeInfo` holds it as a `std::multimap`
member, so the temporary our shim builds destroys it per edge, and `__tree::destroy` is **20%** of
`edgeinfo()` in the profile. Nothing in the borrowed view touches tags. That is an argument for
option 4 on its own - every consumer pays for a cache most never fill.

`kLinguistic` - names, phonetics, languages - is a separate sub-project. `GetNames()` is not
exposed today either, and both go through the same names list.

### Id newtypes

`GraphId` names tiles, nodes and edges, and mixing them compiles: `tile.node(id.id())` and
`tile.directededge(id.id())` are both valid for any id. Every producer is typed - `endnode()` is a
node, `opp_index()` an edge, `locate` returns edges - so `NodeId` / `EdgeId` are mechanically
possible. Cost is friction at the proto boundary, where ids are bare `u64`.

### `locate` and the JSON path

`/locate` has no PBF serializer, so `reachability` and `isochrone-h3` carry the *same* 110-line
serde model. That is the one endpoint where "use proto instead" is not available - and
`proto/descriptors/api.proto` already carries `//TODO: locate;`. Adding that serializer upstream
fits the principles better than adding serde structs to the bindings: the fast path becomes
available, and the duplication disappears on its own. Same question for `height`.

### MSRV

Settled: bump whenever it buys something, do not drift otherwise. The Docker build is
`FROM rust:slim-trixie` with no version pin, so it always has current stable - nothing external
holds the floor down.

Candidate: **1.87 → 1.94** for `slice::element_offset`, which returns `Some(index)` for an element
of the slice and `None` for anything else - precisely the two things this API keeps hand-rolling:

- the tile-membership check, replacing `ref_within_slice()` and its test (16 lines of pointer
  arithmetic) with `self.nodes().element_offset(node).is_some()`. Only inside `debug_assert!` today,
  so it costs nothing in release either way - and its extra modulo also rejects a pointer landing
  mid-element, which `ref_within_slice` accepts
- [`edge_id(de)` / `node_id(node)`](#p2---ergonomics), which examples currently do with index maths,
  become safe one-liners - and there the division is the answer, not overhead

Deferred until that `edge_id` work: it is the release-path use that earns the bump, and swapping the
debug asserts alone buys nothing.

Also adopted, already inside the old floor: `#[expect]` (1.81) on the two `#[allow(deprecated)]`
sites, so they warn the day the deprecated items go - the compiler now drives
[remove deprecated](#6-remove-deprecated).

Considered and not worth a bump on their own: let-chains and `slice::as_chunks` (1.88). Let-chains
would only touch three `match ... Ok(ptr) if !ptr.is_null()` arms, and a `match` with a guard is no
worse. A scan of every std stabilisation from 1.88 to 1.96 turned up nothing else this crate wants.

### Naming consistency

The current API is not actually uniform with C++: `road_class` ← `classification`, `admin_info` ←
`admininfo`, `crosses_country_border` ← `ctry_crossing`, plus `live_traffic` and `edge_speed` which
have no C++ counterpart. Under the mirror-C++ principle, do those get renamed back, or does the
principle only bind new API? (`use_type` has to stay - `use` is a Rust keyword.)

## In tree, uncommitted

`cargo test --all-features`, fmt and clippy green on the crate. Labelled PROTOTYPE where the C++
reaches into `protected` members.

- `src/encoded.rs` - `Shape` and `Elevation` iterators with their unit tests
- `src/lib.rs`, `src/libvalhalla.{hpp,cpp}` - borrowed `EdgeInfo` (way id, speed limit, mean
  elevation, shape, elevation), `DirectedEdge::forward()`, `NodeInfo::elevation() -> Option<f32>`
- `tests/tiles_test.rs` - shape coverage over the whole tileset, plus `edge_profile()` showing the
  assembly a consumer writes
- `benches/api_bench.rs` + its `Cargo.toml` entry - scratch, delete when this lands
- examples: `ferry-lines` (dropped its `polyline-iter` dependency - shape comes out as points now),
  `traffic_debug`, `match-polyline` manifest bumped to `polyline-iter` 0.4

### Half-finished: elevation profile in `reachability`

Started to answer "what does restoring elevation along a path look like", and it is the best
evaluation vehicle - path restoration walks backwards, which is where direction handling bites.
**The example does not compile yet.**

- `src/dijkstra.rs` - done. `search()` returns a `Search` carrying `came_from` breadcrumbs and
  `Search::path()` restores `Vec<PathStep>` in travel order
- `src/profile.rs` - done. `resample()` walks the path emitting elevation every `step` metres,
  interpolating tile samples onto that spacing. Only the output vector is allocated
- `src/main.rs` - **not updated**. Four call sites still expect the old `SearchResult` return
  (lines ~118, 119, 144), and nothing calls `profile::resample` yet

It has already earned its keep as a design test: it forced `NodeInfo::elevation() -> Option`, and it
wants `tile.edge_id(de)` - `dijkstra.rs` reconstructs edge ids by hand with
`GraphId::from_parts(level, tileid, node.edge_index() + i)`, which is the
[P2 ergonomics item](#p2---ergonomics) appearing in real code.

Running it against Andorra will show flat profiles until there is an elevation fixture.
