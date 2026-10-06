# Tile access: what shape replaces `GraphReader`

Exploration, not a proposal yet. Branch `api-tile-source-exploration`. Follow-on from
[a cached `GraphReader`, and the start-node problem](api-freeze.md#a-cached-graphreader-and-the-start-node-problem).

Three shapes were built and run against real workloads rather than argued about:

| | `src/shapes/composed.rs` | `src/shapes/fat.rs` | `src/shapes/capability.rs` |
|---|---|---|---|
| graph access | `TileSource` (1 method) | `GraphReader` (5 methods) | `TileSource` (1 method) |
| enumeration | `TileIndex`, opt-in | defaulted to empty | `TileIndex`, opt-in |
| traffic | standalone `TrafficExtract`, composed in | defaulted to `None` on the trait | `TrafficSource: TileSource` |

Evidence lives in `tests/shapes_test.rs`, `benches/shapes_bench.rs`, and a real port of
`examples/reachability`. Everything below is measured or compiled, not assumed.

**Scope: read-only.** Half the tile-touching work in the wild is writes - every `valhalla_add_*`
tool modifies a `tile_dir`, and a tar cannot be written at all - but the bindings stay read-only.
Historical traffic stays on its existing flow: produce a CSV of per-edge speeds, hand it to the
C++ binary. What the bindings owe that flow is the *read* half - resolving edge ids against the
tiles - which today they cannot do unless the tiles are in a tar.

## The facts that decide it, before any shape

### `tile_dir` is already half-supported, inconsistently

`Actor` constructs a real `baldr::GraphReader(config.get_child("mjolnir"))` (`src/actor.hpp:26`),
which handles `tile_extract`, `tile_dir` *and* `tile_url`. Our `GraphReader` is `tile_extract_t`
only. So for one and the same `Config`:

```rust
Actor::new(&config)        // routes off a tile_dir fine
GraphReader::new(&config)  // Err("Failed to load tile extract")
```

`tests/shapes_test.rs::actor_honours_tile_dir_but_graph_reader_does_not` demonstrates it against
the Andorra fixture unpacked to a directory.

That asymmetry is the production pain directly: map-matching probe data works against a
`tile_dir`, but resolving the matched ids back to edges does not, so a second tar gets built
purely to read. Closing it needs no new concepts - only the backend C++ already has.

### Traffic is two unrelated things

**Read path.** Traffic memory is attached to the `GraphTile` *at construction* -
`GraphTile::Create(base, graph_memory, traffic_memory)` (`src/libvalhalla.cpp`, mirroring
`valhalla/src/baldr/graphreader.cc:610`). After that, traffic is a `GraphTile` method:
`tile.live_traffic(de)`, `tile.edge_speed(..)`. The reader is not involved.

**Write path.** `TrafficTile` is a raw writable mmap handle into a *different* tar -
`write_edge_traffic`, `write_last_update`, `clear_traffic`. `traffic_debug` is the reference
consumer and barely touches the graph.

So *read*-traffic cannot be a reader capability: the source already answered it when it built the
tile, and there is nothing left for a trait to expose. *Write*-traffic is a different object over
a different archive for a different process.

And `graphreader.cc:603-629` shows traffic is joined the same way whether graph tiles come from a
tar **or a directory**. Traffic is not a property of the graph source. Today's `GraphReader`
cannot express `tile_dir` + `traffic.tar`; C++ can.

`tests/shapes_test.rs::a_directory_backend_can_carry_traffic` proves the join works by writing
60 km/h through the traffic archive and reading it back through a directory-backed tile.

### `GraphTile: !Send` decides the thread model, not the trait

The trait carries no `Send`/`Sync` bound and needs none. Auto traits belong to the concrete type:
`TileExtract` is `Send + Sync`, `Cached<_>` is neither, and both implement the same trait. A
consumer that shares states it at the use site:

```rust
fn walk(r: &impl GraphReader)              // takes either
fn parallel(r: &(impl GraphReader + Sync)) // takes TileExtract; Cached is a compile error
```

The compile error is real: *"`RefCell<HashMap<TileId, Option<GraphTile>>>` cannot be shared
between threads safely"*.

**Share the source, never the cache.** Each thread builds its own cache over a shared source -
gitoxide's `Store` / thread-local `Handle`. `thread_model::shared_source_per_thread_cache` does
it over scoped threads. With rayon the idiom is `for_each_init` / `map_init`, whose
`INIT: Fn() -> T + Sync + Send` puts no bound on `T`, so a `!Send` cache per worker is exactly
what it is for.

**This is forced, not chosen.** `TileExtract` is `Sync` *because* it does not cache: every
`graph_tile()` builds a fresh tile with refcount 1, so two threads loading the same tile share
bytes in a read-only mapping and never a refcount
(`thread_model::concurrent_loads_of_one_tile_share_bytes_not_refcounts`). A source that cached
`GraphTile`s and handed out clones would have two threads bumping one non-atomic counter.

C++ hits the same wall and resolves it with a build flag. `GraphTile` derives from
`intrusive_ref_counter<GraphTile, thread_unsafe_counter>`, and `ENABLE_THREAD_SAFE_TILE_REF_COUNT`
swaps `graph_tile_ptr` to `std::shared_ptr`. Its `global_synchronized_cache` is only sound with
the flag on - the mutex guards the map, not the refcounts of the tiles it hands out - yet
`SynchronizedTileCache` is documented as "thread-safe" regardless. We build with the flag off
(`build.rs`) and the plan rejects turning it on (~10% on Actor).

So the rule is: **a shareable source may cache anything except `GraphTile`.** Bytes, indexes,
manifests are fine behind a lock; tiles only in a per-thread cache. For S3 the bytes do not need
sharing either - see [tiles from Rust-owned memory](#tiles-from-rust-owned-memory).

## Measurements

Andorra, 7 tiles, `cargo bench --bench shapes_bench`, `lto = "thin"`.

**Tile load, per tile:**

| backend | per tile | vs extract |
|---|---|---|
| extract (mmap into the tar) | **71 ns** | 1x |
| Rust buffer handed over, build | **108 ns** | 1.5x |
| `tile_dir`, mmap per file | 12.3 µs | 174x |
| bytes the caller supplies, copied | 14-53 µs | allocator-dependent |
| `tile_dir`, Valhalla's own `ifstream` | 82.5 µs | 1160x |
| cached hit | ~11 ns | 0.15x |

Valhalla's directory path is `std::ifstream` into a fresh `std::vector<char>`
(`graphtile.cc:115-122`). Mapping the file instead is **6.7x faster** and lands in the shim as
`graph_tile_from_dir_mmap`. It is still 174x an extract lookup, because it is open + mmap +
madvise + close + munmap per tile - syscalls no design can remove. **A directory backend without
a cache is not viable for traversal**, which is an argument for the crate shipping one.

The 6.7x is also a small, self-contained upstream contribution.

**Joining traffic, per tile load:** 68.4 ns without, 90.8 ns with - **+22 ns, +33%**. The join is
a hash lookup in the traffic index plus a `make_unique<GraphMemory>` per load. That is why
`with_traffic` is opt-in rather than implied: four of the six examples never read live traffic.

Today it is not opt-out. `GraphReader::new` joins traffic whenever the config's `traffic_extract`
resolves, and `ConfigBuilder` defaults that to `/data/valhalla/traffic.tar` - so on a box where
that file happens to exist, every tile load pays 33% whether or not `live_traffic` is ever called.
A cache amortises it away, which is one more way the cache question couples to this one.

**Cache lookup, per lookup (10k lookups over 7 resident tiles):**

| | per lookup | delta |
|---|---|---|
| `&mut self -> &GraphTile` (what `reachability` hand-rolls) | **8.7 ns** | - |
| `&self -> &GraphTile`, append-only (elsa-style) | 15.5 ns | +6.8 |
| `&self -> GraphTile`, `RefCell` + refcount clone | 19.2 ns | +10.5 |
| uncached extract load | 69 ns | +60 |

Components: `GraphTile::clone` + drop is **1.95 ns**, `RefCell::borrow_mut` + map get is
**5.57 ns**.

So `&self` ergonomics cost ~10 ns/lookup. That is 15% of one uncached extract load and 0.012% of
one directory load. **The mutability question is not a performance question.**

**Dispatch, whole 14-hop traversal:** `impl TileSource` 8.49 µs, `dyn TileSource` 8.57 µs -
**0.8%**. Dynamic dispatch is not an argument against anything here.

**Hash key.** A first measurement over 7 resident tiles said `TileId` was 2% *slower*; that was
an artefact of a map small enough to be pure noise. Re-measured at production scale - 205k tiles
in a planet tileset, 2k resident in a realistic LRU against 80 GB (`benches/keybench.rs`):

| entries | `HashMap<TileId, _>` | `HashMap<GraphId, _>` | |
|---|---|---|---|
| 2,000 (realistic LRU) | 268 µs | 361 µs | `TileId` **26% faster** |
| 205,000 (whole planet) | 440 µs | 606 µs | `TileId` **27% faster** |

50k lookups each. The win is hashing 4 bytes instead of 8. `HashSet` build at 205k is 3.25 ms vs
3.68 ms, 13% faster.

Memory, at the same scale:

| | `TileId` | `GraphId` |
|---|---|---|
| `(key, GraphTile)` - the cache itself | 16 B | 16 B |
| 2k-entry LRU | 31 KB | 31 KB |
| `(key, u32)` - visited sets, counters | 8 B | 16 B |
| `Vec` of every tile, 205k | 800 KB | 1,601 KB |

So the cache-memory argument is dead - `(u32, ptr)` pads back to 16 B, and 31 KB at 2k entries was
never going to matter. What survives is `tiles()` at planet scale (800 KB) and any auxiliary
`(tile, small)` map, which halves.

**Unrelated but confirmed while measuring:** building `Vec<GraphId>` of 205k tiles takes 413 µs
against 8.9 µs for `Vec<TileId>`. That is *not* the type - it is `GraphId::from_parts` at 2.0
ns/call because it is an FFI hop returning a `Result`, matching the 1.91 ns the plan already
records. Independent support for the P2 item "pure-Rust `GraphId::from_parts`".

## What each shape cost in real code

### Enumeration: the split was wrong, and `index.bin` is why

Shape A and C put `tiles()` behind an opt-in `TileIndex` on the grounds that a directory or a
remote backend cannot answer it. Both halves of that turned out to be false.

**C++ already answers it for a directory.** `GraphReader::GetTileSet()` uses hash keys for an
extract and `recursive_directory_iterator` for a `tile_dir` (`graphreader.cc:920-950`). Splitting
it off diverges from C++ for nothing - principle 1.

**And the cost is not uniform across the two methods.** `tiles_in_bbox` never needs a walk
anywhere: `TileHierarchy::levels()[level].tiles.TileList(bbox)` computes candidate ids from pure
geometry, and presence is then one hash lookup (extract) or one `stat` (directory). Only `tiles()`
- every tile, no bbox - costs a directory walk.

**Remote can enumerate too, via `index.bin`.** Every extract carries it as the first member, and
it is a complete manifest: `{offset: u64, tile_id: u32, size: u32}` per tile
(`graphreader.cc:24-28`). `GraphTile::CacheTileURL` already takes `range_offset` / `range_size`
"in case of a tar URL". So a remote tar plus one `index.bin` fetch yields the full tile list *and*
the byte range of every tile - enumeration and random access, without downloading the planet.

So enumeration is not a capability a backend has or lacks; it is a cost that varies by backend and
by method. That is OpenDAL's native-vs-full capability distinction, and it is a docs problem, not
a trait-bound problem. **`TileIndex` collapses back into the main trait.**

### Shape B's defaults produce wrong answers, silently

`a_directory_backend_silently_scans_nothing` passes with `count_edges(&DirReader) == 0` against a
directory that holds all 30,418 edges, and `dataset_id() == 0`. No error, no warning, and
indistinguishable from an empty tileset.

The same function under A and C is a **compile error** - verified, not assumed:

```
error[E0277]: the trait bound `TileDir: TileIndex` is not satisfied
```

### B and C both hold the traffic updater hostage

Because `traffic_tile()` sits on the graph trait (B) or behind `TrafficSource: TileSource` (C), a
traffic updater must construct a graph reader it never uses. On a box holding only `traffic.tar`
it cannot start:

```rust
// shape B / C
assert!(ExtractReader::open("/nonexistent/tiles.tar").is_err());  // updater is dead here
let reader = ExtractReader::open(ANDORRA_TILES)?.with_traffic(&path, true)?;

// shape A - traffic is its own axis
let traffic = TrafficExtract::open_writable(&path)?;
for id in traffic.tiles() { traffic.tile(id)?.write_edge_traffic(..) }
```

**Correction, since this was overstated earlier.** The cost is *not* 80 GB of memory: `mmap` is
lazy, so a mapping never read costs address space, not RAM. What it actually costs is (a) the
graph tar must be present and readable on that box, and (b) `load_tiles` builds a 205k-entry index
up front - cheap when the extract carries `index.bin` (one 3.3 MB member), expensive without it,
since the fallback scans every tar header across the whole 80 GB. So the real benefit is
deployment shape - an updater that needs no graph tiles at all - plus the read-only/writable
choice, not a memory saving.

### Shape C cannot express `tile_dir` + `traffic.tar` at all


Traffic being a trait on the source means a traffic-carrying directory backend needs a distinct
`TrafficDir` type, and `with_traffic` has to thread a `writable: bool` through because there is
nowhere else for the read/write choice to live. Shape A composes an already-opened archive, so
the choice is made once, where the archive is opened.

### Porting `examples/reachability` to shape A

`dijkstra.rs`: **131 -> 98 lines**, identical output on every coordinate tested.

21 lines of `CachedGraphReader` deleted - the crate provides it. But the bigger win was
unexpected: the 20-line `visited.entry()` match collapsed to 8 lines. That match only existed to
satisfy the `&mut self` borrow - the old code had to fetch the tile *inside* the match arms and
`.expect()` in the occupied arm. With `&self` and an owned tile, loading the tile and updating the
visited set are independent statements again:

```rust
let Some(tile) = graph_reader.graph_tile(TileId::of(node_id)) else { continue };
let first_visit = visited
    .entry(node_id.tile())
    .or_insert_with(|| BitSet::new(tile.nodes().len()))
    .insert(node_id.id() as usize);
if !first_visit { continue; }
```

`&mut self` did not just cost a struct; it distorted the algorithm's control flow. That is worth
more than the 10 ns it saves.

## The factory does not need the wide trait

The one thing shape B genuinely bought - a config-driven factory returning one object - turns out
not to need it. `TileSource` has a single non-generic method, so it is object-safe on its own;
`Box<dyn TileSource>` was always available.

What a `dyn` loses is enumeration and traffic. Answering that with defaults that lie is shape B's
mistake. `AnySource` (in `composed.rs`) answers it honestly instead:

```rust
let source = AnySource::from_config(&config)?;   // extract, then dir - C++'s fallback order
match source.index() {
    Some(index) => println!("{} tiles", index.tiles().len()),
    None => println!("this backend cannot enumerate"),  // not "0 tiles"
}
```

This is OpenDAL's `Capability` idea with two capabilities instead of forty.

## Tiles from Rust-owned memory

Remote loading stays in Rust - no C++ curl. Rust fetches the bytes and **hands ownership to the
tile**: there is exactly one owner at any time.

```rust
pub fn graph_tile_from_memory(id: TileId, bytes: Vec<u8>) -> Option<GraphTile>;
```

C++ holds the buffer as a `rust::Vec<uint8_t>` inside a `GraphMemory` subclass. A `rust::Vec`'s
heap buffer does not move when the `Vec` does, so the tile reads straight out of it, and
destroying the tile frees it through Rust's allocator. No trait, no `unsafe` in the public API, no
stability contract for callers to uphold. A copy is just `from_memory(id, bytes.to_vec())`, so the
old copy-into-`std::vector` path is gone from the shim.

**Why single ownership, not shared buffers.** An earlier cut took an `unsafe trait TileMemory`
with `Arc<[u8]>` impls so threads could build tiles over one shared buffer. That was memory-safe -
each tile held its own strong reference - but ownership was split across every holder, so a byte
cache evicting an entry freed nothing while some worker's tile still held a clone, and the cache's
size accounting drifted from real memory. And there is no use case for several threads reading one
tile's bytes. One owner means the memory a tile uses is the memory it holds.

**The channel model.** A producer - in production an async S3 range reader, as in `../rati` -
sends `(TileId, Vec<u8>)` to workers. `Vec<u8>` is `Send`, so it crosses the channel; the
`GraphTile` built from it is `!Send`, so it stays on the worker. Ownership goes producer, channel,
worker, tile, and nobody else holds the buffer at any point
(`from_rust_memory::producer_sends_bytes_workers_own_the_tiles`).

That also settles the async question for scanning: the producer is async, the workers are sync,
and the channel is the boundary - no `GraphReader` involved at all. For on-demand traversal over
S3 a worker still has to *ask* for tile X and wait, so the pull path through `graph_tile` blocks on
a miss. That half is still open.

**Getting a `Vec<u8>` from what S3 clients return.** In `bytes` 1.12.1 (what `rati` pins),
`Vec::from(Bytes)` reuses the allocation when the `Bytes` is unique - `Vec`-backed, or Arc-backed
with refcount 1; a sliced one is `memmove`d to the front in place - and copies when it is static,
owner-backed, or still shared. So at worst the producer pays one copy, off the worker.

**Proved, not assumed:**

- *no copy, by address* - the tile's `directededges()`, `nodes()` and an edge's encoded shape all
  point inside the allocation that was moved in (`tile_reads_straight_out_of_the_handed_over_buffer`)
- *the tile is the only owner* - `tests/tile_ownership_test.rs` installs a global allocator that
  watches the buffer's address, and sees it freed on the tile's last clone and not before, freed
  when `Initialize` throws on a truncated tile, and freed when the id check rejects the bytes. It
  also shows the free goes through Rust's allocator, which a mismatched `free()` in C++ would not.

**Cost.** Building over a handed-over buffer is 108 ns per tile against 75 ns for an extract. The
*free* is the expensive part: ~13 µs per tile when the tile drops - but a baseline that frees the
same buffers with no tile involved costs the same, so it is entirely the system allocator returning
200 KB-1.6 MB allocations, not `GraphTile` teardown. Any model that frees the buffer pays it; the
extract avoids it only because a mapping is never freed. Against an S3 round-trip it is noise. If
it ever matters, the allocator is the lever, not this API.

**Trust model.** `Initialize` checks size, `end_offset == size` and version, then walks the
sections by the header's counts without bounding them, and nothing checks record offsets either
(`node_edges` is `edges.data() + node.edge_index()` unchecked). So tile *contents* are trusted on
every path, tar and directory included. Rust-owned bytes do not change that; they make it visible.
Truncation, the realistic failure for a range read, is caught by `end_offset`.

**One check C++ does not do.** `Create` never compares the id it is given with the header's, and
`id()` returns the header's. A fetcher that read the wrong range would get a valid tile silently
reporting another id; `graph_tile_from_memory` returns `None` instead
(`bytes_of_another_tile_are_rejected`).

## Recommendation

One read-only trait carrying the graph surface, with traffic composed in as a separate archive:

```rust
pub trait GraphReader {                       // object-safe
    fn graph_tile(&self, id: TileId) -> Option<GraphTile>;
    fn tiles(&self) -> Vec<TileId>;           // O(walk) for a dir; index.bin for remote
    fn tiles_in_bbox(&self, min: LatLon, max: LatLon, level: GraphLevel) -> Vec<TileId>;
    fn dataset_id(&self) -> u64;              // loads one tile, on every backend
}

struct TileArchive { .. }                     // the index + tar mapping, graph or traffic
pub struct TileExtract { .. }                 // graph tiles over a TileArchive
pub struct TrafficExtract { .. }              // traffic tiles over a TileArchive - own axis
pub struct TileDir { .. }                     // + with_traffic(TrafficExtract)
pub struct Cached<R> { .. }                   // decorator, !Send like every tile cache
pub fn graph_tile_from_memory(id, Vec<u8>)    // takes ownership; any Rust-side backend builds on it
```

This is close to the original instinct - one trait, whole surface - with exactly one correction
that survived the evidence: **traffic comes off it.** Grounded in:

1. traffic on the graph trait makes an updater open a graph archive it never uses - which means
   the graph tar has to exist on that box, and its 205k-entry index gets built for nothing. Not a
   memory cost (mmap is lazy); a deployment-shape cost
2. `tile_dir` + `traffic.tar` is expressible only when traffic is its own axis, and C++ supports
   that combination
3. enumeration stays on the trait: C++ answers it for directories, `tiles_in_bbox` is cheap
   everywhere, and `index.bin` covers remote
4. `dyn` costs 0.8% and `&self` costs ~10 ns, so none of this is a performance trade

Remote stays unbuilt for now. When it lands it is Rust-side - `index.bin` + range reads, as in
`rati` - over `graph_tile_from_memory`, rather than Valhalla's per-tile curl, because that is the
version that can enumerate and the one Rust owns end to end.

Naming, and this is where your instinct was right: under shape A the trait is the thing you reach
for, so **`GraphReader` should be the trait name**, with `TileExtract` / `TileDir` / `FetchSource`
as the implementations. It reads the way a C++ user expects - "the thing you get tiles from" -
without the crate ever shipping the monstrosity. The cost is that a C++ user will look for
`tiles()` and `traffic_tile()` on it and not find them. Open question below.

## Prior art

- **gitoxide `gix-odb`** - the closest analogue. `Store` is shared and thread-safe; `Handle` is
  "a thread-local handle to access any object" owning thread-local caches. Exactly the
  `TileArchive` (Send+Sync) / `Cached` (!Send) split that `GraphTile: !Send` forces on us.
- **OpenDAL** - one `Operator` plus a runtime `Capability` descriptor (`read`, `write`, `list`,
  …), queried via `OperatorInfo::capability`. Notably it distinguishes `native_capability` from
  `full_capability` - things a backend cannot do natively but can emulate. That maps onto a
  future `TileDir::scan()`: enumeration is not native to a directory, but is achievable at a cost
  the caller should opt into.
- **`object_store`** (Arrow) - one wide `ObjectStore: Send + Sync` trait, unsupported operations
  return `Error::NotSupported`. Shape B done properly: the lie is an *error*, never an empty
  value. If we ever go wide, this is the version to copy.
- **tantivy** - narrow `Directory` trait, caching as a separate decorator. Shape A's structure.
- **`elsa::FrozenMap`** - `&self` insert returning `&V`, sound because values are boxed and never
  removed. Benchmarked above at 15.5 ns; worth revisiting if the 10 ns ever matters, and it is
  what every example's non-evicting cache already is.
- **`moka` / `quick_cache`** - `&self` caches are the norm in Rust; `&mut self` is the outlier.

## Found on the way

1. **Read-only traffic mappings are a constraint on the expansion, not a bug today.**
   `TrafficTile`'s write methods go straight into the mmap, so on a read-only mapping they
   SIGBUS. This cannot happen on `main`: `GraphReader::new` hardcodes `tile_extract_t(pt, false)`,
   so every traffic mapping is writable. It only becomes reachable because *this* exploration
   offers `open_traffic` alongside `open_traffic_writable`. So it is a rule the expansion has to
   carry - **any interface exposing the write methods must have opened the archive writable** -
   and the implementation adjusts to match, rather than something to fix on `main`.
   `tests/shapes_test.rs::writing_to_a_readonly_traffic_mapping_is_a_sigbus`, `#[ignore]`d because
   it kills the process. Cheapest enforcement is a separate read-only handle type, so the write
   methods simply do not exist on it; a `writable: bool` parameter (shape C) does not enforce
   anything.
2. **`LiveTraffic::UNKNOWN` is not the only "no reading" value.** A tile built with no traffic
   mapping reports `0x0FFFFFFF`, Valhalla's static invalid record. Both decode to
   `speed() == None`, so behaviour is right, but `== LiveTraffic::UNKNOWN` is the wrong test and
   nothing says so.
3. **`ConfigBuilder` gives every path a non-empty default** - `tile_dir: "/data/valhalla"`,
   `tile_extract: "/data/valhalla/tiles.tar"`, `traffic_extract: "/data/valhalla/traffic.tar"`. A
   config-driven factory therefore cannot distinguish "the user asked for this" from "Valhalla's
   default", and must probe the filesystem exactly as C++ does. Caught by the ported example
   failing with `Failed to load traffic extract`.
4. **`Config` was write-only** - no getters at all, so no factory could read back what it was
   handed. Added `Config::get_str` and a `ptree_get_str` shim.
5. **Valhalla's `tile_dir` load is 6.7x slower than it needs to be** (`ifstream` + `vector` vs
   mmap). Upstream-able on its own.

## Open questions

- Should the trait be named `GraphReader` (matches intuition, but a C++ user will hunt for
  `tiles()` on it) or `TileSource` (honest, unfamiliar)?
- Does `TileDir` get a `scan()` that walks the tree and returns a `TileIndex` view, OpenDAL's
  emulated-capability style? It is the only way a directory backend can enumerate.
- Should `TrafficTile` become `Send`? Its pointers are into a shared mmap and its accessors are
  already volatile and coherence-safe; `!Send` looks like an artefact of the raw pointers rather
  than a real constraint, and it blocks a multi-threaded traffic updater.
- Does the crate ship an *evicting* cache, or only the unbounded `TileMap`? Every example gets
  away with unbounded because its search is bounded by other means - but a directory backend at
  planet scale would not, and mmap-per-file also means one VMA per resident tile.
- `Cached::edge_nodes()` works (`edge_nodes_resolves_both_ends`) and is the start-node helper the
  plan wanted. Does `edge_elevation` follow it once `edgeinfo-elevation` lands?

## State of the branch

`cargo test --all-features` 83 passing, `cargo clippy --all-features --all-targets` clean,
`cargo fmt` applied. `examples/reachability` ported and verified byte-identical on four
coordinates.

Everything under `src/shapes/`, `tests/shapes_test.rs` and `benches/shapes_bench.rs` is scratch -
delete it once a shape is chosen. The shim additions (`TileArchive`, the three
`graph_tile_from_*` families, `tile_file_suffix` / `tile_id_from_path`) and `Config::get_str` are
the parts worth keeping under any outcome.
