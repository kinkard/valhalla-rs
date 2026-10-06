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
before quoting them. On this box, in the state described here, `way_id` over the tileset is
162.6 µs (`benches/edgeinfo_bench.rs`, no cross-language LTO).

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
- [x] elevation as the edge's full profile - see [Elevation](#elevation)
- [x] `DirectedEdge::forward()` exposed. It was never bound, which is the only reason the prototype
      copied it into the view - a per-edge fact on a type both edges of a pair share
- [x] `NodeInfo::elevation()` returns `Option<f32>` instead of a `-500.0` sentinel. Needed the
      moment anything consumes elevation
- [x] `GraphTile::has_elevation()` from the tile header - the only reliable "was elevation built"
      signal, see [Elevation](#elevation)
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

Re-measured at production scale while exploring the reader shape - 205k tiles in a planet
tileset, 2k resident in a realistic LRU against 80 GB (a scratch bench on
`api-tile-source-exploration`, since removed). An earlier 7-entry measurement said `TileId` was 2% *slower*; that
was a map small enough to be pure noise, and it was wrong:

| lookups over | `HashMap<TileId, _>` | `HashMap<GraphId, _>` | |
|---|---|---|---|
| 2,000 entries (realistic LRU) | 268 µs | 361 µs | **26% faster** |
| 205,000 entries (whole planet) | 440 µs | 606 µs | **27% faster** |

50k lookups each; the win is hashing 4 bytes instead of 8. `HashSet` build at 205k is 3.25 ms vs
3.68 ms. So there is a speed case after all, and it shows up at exactly the sizes that matter.

The memory case narrowed, though. At 2k resident the cache is 31 KB either way, so cache size was
never the argument - only `tiles()` at planet scale (800 KB) and auxiliary `(tile, small)` maps,
which halve.

**The real argument is a silent bug class.** `GraphId` hashes and compares the full u64, id bits
included, so against a `HashMap<GraphId, GraphTile>` both of these compile:

```rust
cache.get(&edge_id)          // every edge is a distinct key - 100% miss, unbounded growth
cache.get(&edge_id.tile())   // correct
```

Shipping the cache in the crate removes that instance but not the class: of the 24 `.tile()` call
sites in `examples/` and `tests/`, only the cache keys are absorbed. `reachability`'s
`visited: FxHashMap<GraphId, BitSet>` is a visited set, not a cache; `isochrone-h3` fuses cache and
visited into one map on purpose ("one hash map as both tile cache and visited set") and so cannot
use a shipped cache at all; and `ferry-lines` compares `end_node_id.tile() == begin_tile.id()`
three times, where dropping `.tile()` compiles, is always false, and silently reloads the tile per
edge.

**Settled shape: `TileId` in return position, `GraphId` still accepted at call sites.** On
ergonomics alone one id type wins - fewer types, no conversions, and it mirrors C++, which is
principle 1. The asymmetric version costs no ergonomics and keeps the check where it pays:

```rust
fn tiles(&self) -> Vec<TileId>;   // cannot be mistaken for node ids
fn id(&self) -> TileId;           // on GraphTile - makes the comparison type-checked
reader.tile(node_id)              // GraphId, reads exactly as today
```

Generic `impl Into<TileId>` arguments would break object safety and with it any `dyn` reader, so
they go on a blanket-implemented extension trait while the object-safe method stays on the trait -
the `Iterator`/`Itertools` pattern. Verified working together in
`tests/breaking_test.rs::tile_id_ergonomics`.

- [ ] `TileId`; `GraphId::tile() -> TileId`, `tiles() -> Vec<TileId>`, `GraphTile::id() -> TileId`
- [ ] `GraphReaderExt`-style convenience so call sites keep taking `GraphId`
- [ ] decide whether `NodeId` / `EdgeId` follow - see [Id newtypes](#id-newtypes)

**Deferred until the reader shape is settled** - see [tile-source.md](tile-source.md). The coupling
is one method signature plus the cache's key type, both internal; call sites do not change either
way, so this is a mechanical follow-up rather than a prerequisite.

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
It has come back, as a trait rather than a cache - see
[a cached `GraphReader`](#a-cached-graphreader-and-the-start-node-problem).

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

Settled, and verified against real bytes. `tests/andorra/tiles.tar` is now built **with**
elevation - Andorra runs from the Valira valley at 844.5 m to the Pyrenean ridge at 2915.25 m,
17,922 of 31,404 edges carry samples, 138,250 samples in all. The tar grew 2,949,120 → 3,123,200
bytes (+5.9%), and traffic.tar had to be regenerated with it. `scripts/build-elevation-fixture.sh`
rebuilds both, and the four non-obvious things it has to do are documented in its header - two of
them are upstream bugs, see below.

Format, re-derived from the encoder, the decoder and its one production caller
(`triplegbuilder.cc` `SetElevation`):

- `int8_t` deltas at 0.25 m, pointer set last in `EdgeInfo`'s constructor - after the shape and the
  optional extended way-id bytes
- count is `encoded_elevation_count(edge->length())`, which is exactly `length / 32`: the three
  float divisions cancel. Verified for every length 1..5,000,000, zero mismatches, and asserted
  against the fixture for all 31,404 edges
- the chain is anchored **only** at the storage-start node. The end node's elevation comes from
  `NodeInfo` and is *not* derivable from the deltas
- **both end nodes are excluded** from the samples (`elevation_encoding.h:42`, `:53`), and
  `decode_elevation` returns `n + 2` values in travel order: `[h1, interior…, h2]`. So the samples
  and the grid are not the same thing, and the grid is uniform: spacing `length / (n + 1)`,
  positions `i * length / (n + 1)` for `i` in `0..=n + 1`

One accessor:

- `EdgeInfo::elevation(edge, start, end) -> Elevation<'a>` - the whole grid, `n + 2` heights in
  travel order, `Elevation::interval()` metres apart, `start` first and `end` last.
  `DoubleEndedIterator` for a path walked backwards, plus `Elevation::reversed()` for when the
  direction is only known at runtime (`Iterator::rev()` changes the type and drops `interval()`).
  The sum reverse iteration needs is computed once, lazily, so forward-only iteration never pays.

Plus the public `encoded_elevation: &[i8]` field, the raw escape hatch, which is *not* redundant
with the accessor - the field is bytes, the accessor applies the 0.25 m scale.

**Why the placed one, and why only it.** The open question was whether to hand back only the
relative samples and let the caller add the absolute height, since the anchor is the edge's start
node when `forward()` and its end node otherwise - so the original `elevation(edge, start, end)`
used one argument and discarded the other. Four shapes were built and driven by the same two
consumers, "resample a path every 50 m" and "height at an exact point on one edge":

| | per-edge lines, path consumer |
|---|---|
| relative samples, storage order, caller anchors | 13 |
| absolute, travel order, `n` interior samples (the original shape) | 10 |
| absolute, travel order, `n + 2` heights | 7 |
| the above, and the caller never writes the interval formula | 4 |

A consumer writing the whole path walk later confirmed the last row from the other direction: of
everything it needed, `interval()` was "the one thing I did *not* have to compute".

The discarded argument was a real defect, but removing it was the wrong fix. Returning `n + 2`
makes both node elevations *load-bearing* - they are the first and last points yielded - so nothing
is wasted, and the caller stops writing `length / (n + 1)`, which is the off-by-one this API kept
inviting: the relative and interior-only shapes spell it `n + 1`, the full-grid one `len() - 1`.
The short-edge case falls out for free: under 32 m there are no samples, and the profile is the
straight line between the nodes, which is what `decode_elevation` does with `interval = length`.

**A second accessor was tried and removed.** `elevation_raw()` returned the `n` stored samples
relative to the anchor node, in storage order. It shipped for one iteration, then five agents were
pointed at the question - two arguing the case adversarially, two writing real consumers for the
target use cases, one auditing for hidden magic. All four evidence-bearing ones converged against
it:

- **It is a correctness trap.** The `n` interior samples span only `n - 1` of the profile's
  `n + 1` segments, and both missing ones adjoin intersections, where grade changes most. Measured
  over the fixture: relative samples alone capture 88.5% of the tileset's 672,636 m of climb, and
  **0.0%** on the 13,482 edges (42.9%) that store nothing at all, where gradient came within 10%
  of the truth for 0 of 11,878. Nothing in the type signals any of it.
- **It saved its own reference consumer nothing.** `walk_elevation_raw` fetched the far node on
  every edge anyway, because on a reverse edge the anchor *is* the far node.
- **It was not the fast path.** 15.6 ns/edge against 14.6 for the public `encoded_elevation`
  field, so its only unique value was the direction handling.
- **Its one plausible constituency does not exist.** The case for it was a sweep over
  `tile.directededges()` computing something frame-invariant, measured 2.20x faster - but the
  statistic that sweep computes is the one the first bullet shows is wrong.

`RawElevation` stays as a `pub(crate)` type, because factoring it out of `Elevation` removed a
field and two branches: `Elevation` now orients it once at construction and both `next()` and
`next_back()` are a straight delegation. What replaced the accessor is not more API - it is
`examples/elevation-profile`, which carries the consumer-side code with its own tests.

Also weighed and not adopted: `rises(edge, start, end)`, yielding the `n + 1` *differences*
instead of heights. It is strictly better than `elevation_raw()` was - correct on every edge,
frame-free except one scalar, and 24.3 against 31.1 ns/edge for an exact ascent/descent scan, a
22% saving. Recorded rather than shipped because one accessor was the goal and the saving is
modest; adding it later is not a breaking change.

**The off-by-one that is left, and it is the important one.** Because each edge's profile
*includes* both of its nodes, and consecutive edges *share* a node, a path-level consumer that
concatenates profiles emits every interior node twice. Upstream hits this too and handles it the
same way - `tyr/serializers.h`, "Iterate through the edge elevation (skip the first)". So the
idiom is: **every edge after the first skips its first point.**
`tests/elevation_test.rs::path_emits_each_shared_node_once` guards it, and it has teeth - deleting
the `.skip()` makes it fail with 109 points instead of 99, one extra per shared node on an
11-edge path. This is the one thing the API cannot make safe on its own, so it is what the doc
comment on `elevation()` should say out loud.

Elevation still rides **in the view** rather than behind a lazy FFI call: in the view costs
~1 ns/edge for a second `rust::Slice` construction, lazy cost 4-5 ns to rebuild `baldr::EdgeInfo`.

**`forward` and `length` stay out of the view.** `elevation(edge, …)` re-takes an edge the view was
already built from, and passing the pair's other edge would silently reverse the result. Carrying
`forward: bool` and `length: u32` in the shared struct fixes that and shortens the call to
`elevation(start, end)`; both fit in existing padding, so `size_of::<EdgeInfo>()` stays 56 either
way. Measured on Andorra (`benches/edgeinfo_bench.rs`, run-to-run drift ±0.3%):

| | `de` parameter | in the view |
|---|---|---|
| `way_id` over the tileset | 162.6 µs | 170.6 µs (+4.9%) |
| elevation profile over the tileset | 365.2 µs | 343.4 µs (−6.9%) |

So it taxes every `edgeinfo()` call, including the `way_id`-only read the whole lazy-view exercise
exists to make fast, to save a caller that asked for elevation two field reads it needs anyway
(`length`). Principle 3 decides it: the parameter stays.

**Verified against real bytes.** `scripts/check-elevation.py` dumps every decoded height with the
lat/lon it was sampled at - replicating `uniform_resample_spherical_polyline` to place them - and
compares against Valhalla's own `/height`, which reads the HGT tiles through skadi. 400 edges in
both directions, 6,948 samples, split by whether the encoder sampled terrain at all:

| | median | p90 | max | within 2 m |
|---|---|---|---|---|
| decoded, terrain-sampled (338 edges) | **0.120 m** | 0.310 m | 0.660 m | 100% |
| decoded, bridge/tunnel/ferry (62 edges) | 7.700 m | 72.250 m | 159.850 m | 20.9% |
| control: profile reversed | 7.380 m | 59.410 m | 186.020 m | 20.1% |
| control: grid off by one sample | 1.670 m | 4.850 m | 17.340 m | 56.7% |

Both controls exist because a check that cannot fail proves nothing; the off-by-one one is 14×
worse at the median and 16× at p90, so the sample grid is right and not merely plausible. The
terrain population's worst error over all 338 edges is 66 cm - quantisation plus this script's own
resampling.

The second row is not a decode error, it is the data: `encode_btf_elevation` gives bridges,
tunnels and ferries a straight line between their two ends, so what is stored for a tunnel is not
the mountain above it, and `/height` disagrees by up to the depth of the mountain. Splitting the
two populations is what turned a confusing p90 of 14.79 m into the table above.

Three properties a consumer cannot see from the API, all worth documenting:

- one step is capped at +31.75 / -32.0 m, and the encoder **clamps and carries** the remainder into
  the next delta, so a decoded profile can trail the true one across something very steep. An error
  is only flagged past ±256 (±64 m). Andorra has 2 such edges, which the build warns about
- missing elevation is stored as a *zero delta*, not a gap, so NO_DATA decodes as "same as the
  previous sample" and is indistinguishable from genuinely flat ground
- bridges, tunnels and ferries are interpolated, not sampled - as measured above

**`GraphTile::has_elevation()`** is bound (`graphtileheader.h:135`). It is the only reliable "was
elevation built" signal, because `EdgeInfo::mean_elevation`'s sentinel is not - see the upstream
list. `NodeInfo::elevation()` returns `Option<f32>` off the `-500.0` sentinel, which *is* reliable
for nodes: only `elevationbuilder` writes them, and it writes every node in every tile it touches.

**Open: no fixture exercises the no-elevation path any more.** `has_elevation() == false`,
`NodeInfo::elevation() == None` and an empty delta array used to be covered by Andorra itself. The
unit tests in `src/encoded.rs` cover the empty-profile arithmetic, but the three tile-level
behaviours are now untested. Cheapest fix is a second, tiny fixture built from one of Valhalla's
20 KB test extracts with the elevation stage skipped.

### Upstream fixes this work wants

Recorded here because the bindings deliberately do **not** work around them.

1. **`shortcutbuilder` stores `0` as mean elevation instead of `kNoElevationData`.**
   `graphbuilder.cc:940` passes `kNoElevationData` when there is none, but
   `shortcutbuilder.cc:498` passes a plain `0`, which decodes to a perfectly plausible **0 m**. So
   `EdgeInfo::mean_elevation == -500.0` is not a usable "no data" test on a shortcut edge. A
   normalisation in the binding was written and then reverted - the right fix is one constant
   upstream. Until then, `GraphTile::has_elevation()` is the guard.

   Audited against the shipped fixture, this does *not* manifest - 818 shortcuts, means
   846..2290 m, none reading `0.0` or `-500.0` - so it is build-order dependent and nothing in the
   repo tests either branch. A latent wart, not a live bug.
2. **The elevation stage cannot be run on a finished tileset.** `elevationbuilder` re-serialises
   each tile through `GraphTileBuilder` and drops the bin index that `graphvalidator` writes;
   re-running `validate` afterwards does not recover it, and reports "No opposing edge" on a
   cross-tile edge. The tileset then reads correctly - every shape and way id assertion passes -
   but loki finds nothing, so every route fails with "No suitable edges near location". This is
   why the fixture is rebuilt from the PBF rather than patched in place, and it also means
   `valhalla_add_elevation` cannot be used to add elevation to an existing graph.

   Adjacent: `GraphTileBuilder` does not round-trip across minor versions either. 3.8.3's
   elevation stage on a 3.7.0-built tileset produces exactly the same failure, and the header's
   version check only compares the major digit, so nothing catches it.
3. **`tyr::update_bridge_elevations` is off by two.** It divides the rise by `last - first`, but
   its anchors sit at `first - 1` and `last + 1`, two intervals further apart - so the last point
   it rewrites overshoots by `(after - before) / (last - first)` and leaves a step at the far end
   of the run. That step is the artefact the function exists to remove. Found by porting it
   faithfully into `examples/elevation-profile`: a run of three points between anchors at 200 m and
   600 m came out `400, 600, 800` instead of `300, 400, 500`. The example anchors on the indices it
   actually uses and documents the divergence rather than mirroring the bug.
4. **`elevationbuilder` has no zero-length guard.** `encoded_elevation_count(0)` divides 0 by 0.
   The plain path survives by accident - the degenerate resample yields one point, `encode_elevation`
   returns empty, and the elevation flag is never set - but `encode_btf_elevation` reaches
   `static_cast<uint32_t>(NaN)` before it sizes a vector, so a zero-length bridge, tunnel or ferry
   edge is undefined behaviour. Zero-length edges are representable: `set_length` clamps only the
   upper bound, and the 1 m floor is `kMinimumEdgeLength`, a constant local to
   `DirectedEdgeBuilder` that `convert_transit` (co-located platform and station) and `bssbuilder`
   bypass. `thor/route_matcher.cc:60` already tests for them. None in the Andorra fixture: 31,404
   edges, minimum 1 m.
5. **`find_elevation` in `triplegbuilder.cc` clamps only the far end.** A negative distance reaches
   `static_cast<uint32_t>` of a negative double, which is undefined behaviour. Not reachable from
   Valhalla's own caller, but it is the reference anyone copies.
6. **Public accessors for `encoded_shape_` / `encoded_elevation_`**, which delete the
   pointer-to-member shim - same PR as the tags iterator, see [Tag values](#tag-values).

### Resampling, and why none of it ships

A path profile resampled onto a fixed step is what a consumer actually wants, and Valhalla does it
twice - `tyr::get_elevation` for the path, `triplegbuilder.cc`'s `find_elevation` for a single
point. Both were ported and compared against a from-scratch version on the same real path:

| step | split shape | `tyr::get_elevation` port | worst difference |
|---|---|---|---|
| 50 m | 59 points | 60 points | **0.000 m** |
| 25 m | 118 points | 119 points | **0.000 m** |
| 100 m | 30 points | 31 points | **0.000 m** |

So this is not a numerical question - the real difference is 2e-4 m from accumulation order. Two
findings came out of it instead:

**The split shape is the one to write.** Valhalla's is one fused state machine carrying `distance`,
`remaining` and `prior_elevation`, with two intervals in scope at once - the requested step and the
edge's own spacing, which varies **7.0 to 31.5 m** within a single path. Separating "walk the path
emitting `(distance, height)`" from "resample a monotone stream" turns the shared-node off-by-one
into a single visible `skip(1)`, which is also what upstream needs and comments. The first attempt
here was fused, and it had exactly that bug. The one deliberate divergence: Valhalla appends the
path's final point off-grid, so its output is "every step, plus the endpoint" - hence the +1 above.

**A correct profile needs a fix-up nobody would guess.** A tunnel's stored elevation is a straight
line between its portals, but the *node* between two consecutive tunnel edges carries a `NodeInfo`
elevation sampled at the **surface** - on top of the mountain. So `elevation()` faithfully reports
a spike at every intermediate node of a bridge or tunnel run, and the run has to be straight-lined
afterwards. Andorra has **206** such runs, worst middle-node spike **13.8 m** (a 111 m + 101 m run
with portals at 926.0 / 942.8 m whose middle node reads 921.0 m against a straight line at
934.8 m); through an Alpine ridge it is hundreds of metres.

None of this ships in the bindings. The step, the endpoint policy and the interpolation are caller
policy, and walking the path needs the reader. It lives in `examples/elevation-profile` instead,
which is also where the fix-up's upstream off-by-two was found. Left open: an `ElevationProfile`
accumulator - `new(step)` / `push(info, edge, start, end)` / `finish()` - would absorb the four
things every consumer must get right (skip-the-first, the grid, the endpoint policy, the fix-up)
and needs no reader or cache, so it could ship without waiting on those. The fix-up only has to
hold back the points inside the current run plus one, so bounded buffering would do it; that has
not been built or proved.

### Field or method

Not opened by elevation, but sharpened by it. `EdgeInfo` is a cxx shared struct, so `way_id`,
`speed_limit`, `mean_elevation` and `encoded_shape` are public fields, while `shape()` and
`elevation()` are methods. C++'s `baldr::EdgeInfo` is all methods - `wayid()`, `speed_limit()`,
`mean_elevation()`, `shape()` - so under principle 1 the fields are the deviation, not the methods.

Making the public `EdgeInfo` a newtype over the shared struct would make every accessor a method,
let `mean_elevation()` return `Option<f32>` like `NodeInfo::elevation()`, and leave room to change
the representation later. A `#[inline(always)]` field read costs nothing. Cost is churn across
every call site and both examples.

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

- `src/encoded.rs` - `Shape`, `RawElevation` and `Elevation` iterators with their unit tests
- `src/lib.rs`, `src/libvalhalla.{hpp,cpp}` - borrowed `EdgeInfo` (way id, speed limit, mean
  elevation, shape, elevation), `DirectedEdge::forward()`, `NodeInfo::elevation() -> Option<f32>`,
  `GraphTile::has_elevation()`, `NO_ELEVATION_DATA`
- `tests/andorra/{tiles,traffic}.tar` - **rebuilt, now with elevation.** Also a newer OSM extract,
  so the counts moved: 30,418 → 31,404 edges, 13,153 → 13,577 nodes, 353,806 → 366,708 shape
  points, `dataset_id` 12953172102 → 14133604149, and three `expansion_test` distances. traffic.tar
  had to be regenerated to match the new edge count
- `tests/tiles_test.rs` - shape coverage over the whole tileset, plus
  `edge_info_elevation_is_stored_per_32_meters` asserting the sample count for all 31,404 edges
- `tests/elevation_test.rs` - what the crate promises about one edge's profile: it matches the
  stored samples and mirrors on the opposite edge over all 31,170 in-tile edges, it is a uniform
  grid, a sub-32 m edge is the straight line between its nodes, plus the ground-truth dump
- `examples/elevation-profile/` - the consumer side, with its own tests: recovering both end nodes
  from an edge id, stitching a path, resampling onto a fixed grid, flattening tunnel runs, and the
  point lookup. Five tests, one per learning
- `scripts/build-elevation-fixture.sh`, `scripts/check-elevation.py` - rebuild the fixture, and
  re-run the ground-truth comparison against Valhalla's own sampler. The first one carries the
  four things about building a tileset with elevation that are not obvious
- `benches/edgeinfo_bench.rs` - scratch, but it is what settled `forward`-in-the-view; delete with
  the rest of the benches
- fixed while auditing the above, all outside elevation proper: a **soundness hole** where
  `encoded_elevation_count(0)` divides 0 by 0 and yields a 4 GB `rust::Slice` readable from safe
  Rust; `GraphId` gaining `Ord`/`PartialOrd`, which three separate consumers hit as a *compile
  error* reaching for `BinaryHeap` or `BTreeSet`; `Shape::count()` violating the `Iterator`
  contract on truncated input; `Shape::size_hint` under-reserving 3x so `collect()` reallocated
  twice per edge; `Elevation::reversed()`, the runtime-direction escape that only the removed type
  had; and `NodeInfo::elevation()` crossing the FFI boundary twice
- `benches/api_bench.rs` + its `Cargo.toml` entry - scratch, delete when this lands
- examples: `ferry-lines` (dropped its `polyline-iter` dependency - shape comes out as points now),
  `traffic_debug`, `match-polyline` manifest bumped to `polyline-iter` 0.4

### Not in tree: elevation profile in `reachability`

This section previously described `src/profile.rs` and a reworked `src/dijkstra.rs` in
`examples/reachability`. **Neither exists on `release-0.7.0` or on `edgeinfo-backup`** - the plan
was ahead of the tree. What it was meant to prove is now covered by `tests/elevation_test.rs`,
which walks a real path and resamples it, so the example is no longer load-bearing for the API
decision.

Still worth writing for one thing the tests do not cover: `dijkstra.rs` reconstructs edge ids by
hand with `GraphId::from_parts(level, tileid, node.edge_index() + i)`, which is the
[P2 ergonomics item](#p2---ergonomics) - `tile.edge_id(de)` - appearing in real code.

### A cached `GraphReader`, and the start-node problem

`tests/elevation_test.rs::elevation_at_a_point` needs both end nodes of an edge it has only the id
of, and that takes about 15 lines: a `graph_tile` for the end node, `opp_index` into its edge list,
then possibly another `graph_tile` for where the opposite edge ends - because `DirectedEdge` has an
`endnode()` and no start node. Every consumer that is not already walking a path writes them.

The obvious helper - `GraphReader::edge_nodes(edge_id)` - is not really blocked on API design. It
is blocked on the same thing as [a tile cache in the crate](#rejected): resolving the start node
*loads tiles*, and `graph_tile()` costs 74 ns against 2 ns to clone a cached one, so a helper that
silently loads two tiles per call is a trap unless there is somewhere to cache them. That is why
every example hand-rolls a cache, and why they can get away with it - their searches are bounded by
other means.

So the direction to reconsider is not the helper but the reader: a **trait over tile access** plus
one or more cached implementations, at which point `edge_nodes`, `edge_profile` and the
"simple" half of the elevation API (see below) all become reasonable to offer. Valhalla's own
bounded `TileCacheLRU` is the implementation to reach for rather than a new Rust map, and the trait
is what keeps the eviction policy a caller's choice instead of the crate's.

Probably not this iteration - it is a bigger change than anything else on this list, and it wants
its own design pass. But it is the thing standing between the current API and the two-tier shape
that keeps coming up:

- **raw**, `EdgeInfo::elevation_raw()` - the stored samples, no frame, no loads
- **placed**, `EdgeInfo::elevation(edge, start, end)` - the whole profile; no loads either, because
  the caller already has both nodes from walking the path
- **resolved**, what needs the cache: `reader.edge_elevation(edge_id)` - finds both nodes itself,
  for the caller that has an edge id and nothing else

The first two ship. Adding the third later is not a breaking change, so freezing without it is
safe.
