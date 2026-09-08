//! Pins the allocation behaviour the README promises.
//!
//! Resolving a `#/components/...` pointer must not allocate, however long the
//! chain. Pointers that carry an escape are the documented exception: decoding
//! them has to build the decoded name.

use openapiv3::{OpenAPI, PathItem, Schema};
use openapiv3_resolve::Resolve;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

struct Counting;

// SAFETY: every method forwards to `System`, which upholds the contract; the
// counter only observes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn allocations_during(body: impl FnOnce()) -> usize {
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    body();
    ALLOCATIONS.load(Ordering::Relaxed) - before
}

fn spec() -> OpenAPI {
    serde_json::from_str(
        r##"{
          "openapi": "3.0.0",
          "info": { "title": "t", "version": "1" },
          "paths": { "/pets": { "get": { "responses": {} } } },
          "components": {
            "schemas": {
              "A": { "$ref": "#/components/schemas/B" },
              "B": { "$ref": "#/components/schemas/C" },
              "C": { "title": "C", "type": "string" }
            }
          }
        }"##,
    )
    .expect("fixture spec parses")
}

// One test, not two: the counter is process-wide, so a second test running
// concurrently would be measured as this one's allocations.
#[test]
fn component_pointers_allocate_nothing_and_escaped_ones_allocate_to_decode() {
    let openapi = spec();

    let component = allocations_during(|| {
        for _ in 0..1_000 {
            let resolved = openapi.resolve_ref::<Schema>("#/components/schemas/A");
            assert!(resolved.is_ok());
        }
    });
    assert_eq!(component, 0, "a multi-hop component resolve allocated");

    // The documented exception. If this ever reaches zero the README claim can
    // be strengthened; until then it must stay qualified.
    let escaped = allocations_during(|| {
        let resolved = openapi.resolve_ref::<PathItem>("#/paths/~1pets");
        assert!(resolved.is_ok());
    });
    assert!(
        escaped > 0,
        "expected the escape to be decoded into a new name"
    );
}
