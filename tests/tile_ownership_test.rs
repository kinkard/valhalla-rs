//! A tile built from a handed-over buffer is that buffer's only owner: it frees it exactly when
//! its last clone drops, and right away when construction fails - and it frees it through Rust's
//! allocator, not C++'s. Proved by watching the global allocator for that one address.
//!
//! Its own test binary, and one test, because it installs a global allocator.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use valhalla::breaking::{TileArchive, TileId, graph_tile_from_memory, tile_file_suffix};

struct Watch;

static WATCHED: AtomicUsize = AtomicUsize::new(0);
static FREED: AtomicBool = AtomicBool::new(false);

unsafe impl GlobalAlloc for Watch {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr as usize == WATCHED.load(Ordering::SeqCst) {
            FREED.store(true, Ordering::SeqCst);
        }
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Watch = Watch;

fn watch(bytes: &[u8]) {
    FREED.store(false, Ordering::SeqCst);
    WATCHED.store(bytes.as_ptr() as usize, Ordering::SeqCst);
}

fn freed() -> bool {
    FREED.load(Ordering::SeqCst)
}

const ANDORRA_TILES: &str = "tests/andorra/tiles.tar";

fn tile_bytes(id: TileId) -> Vec<u8> {
    let out = std::process::Command::new("tar")
        .args(["-xOf", ANDORRA_TILES, &tile_file_suffix(id, false)])
        .output()
        .expect("tar");
    assert!(out.status.success() && !out.stdout.is_empty());
    out.stdout
}

#[test]
fn the_tile_is_the_only_owner() {
    let ids = TileArchive::open_graph(ANDORRA_TILES).unwrap().tiles();
    let (a, b) = (ids[0], ids[1]);

    // Freed on the last clone, not the first.
    let bytes = tile_bytes(a);
    watch(&bytes);
    let tile = graph_tile_from_memory(a, bytes).unwrap();
    let clone = tile.clone();
    drop(tile);
    assert!(!freed(), "a clone is still alive");
    drop(clone);
    assert!(freed(), "the last clone frees it, through Rust's allocator");

    // Freed when `Initialize` throws: the buffer is already inside the half-built tile by then.
    let mut bytes = tile_bytes(a);
    bytes.truncate(bytes.len() / 2); // keeps the allocation, so the address still matches
    watch(&bytes);
    assert!(graph_tile_from_memory(a, bytes).is_none());
    assert!(freed(), "not leaked when construction fails");

    // Freed when the bytes are a valid tile, but not the one asked for.
    let bytes = tile_bytes(a);
    watch(&bytes);
    assert!(graph_tile_from_memory(b, bytes).is_none());
    assert!(freed(), "not leaked when the id check rejects it");
}
