//! Pins the allocation behaviour the README promises.
//!
//! Resolving a `#/components/...` pointer must not allocate, however long the
//! chain. Pointers that carry an escape are the documented exception: decoding
//! them has to build the decoded name.

use openapiv3::{OpenAPI, PathItem, Schema};
use openapiv3_resolve::Resolve;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    // Per thread, so allocations the test harness or a concurrently running
    // test make elsewhere are never attributed to the body being measured.
    // `const` initialisation keeps the TLS access itself allocation-free,
    // which matters because it runs inside the allocator.
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

fn record_allocation() {
    // `try_with` fails only while this thread's TLS is being torn down, when
    // nothing can be measuring any more, so there is nothing to count.
    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
}

struct Counting;

// SAFETY: every method forwards to `System`, which upholds the contract; the
// counter only observes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_allocation();
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Allocations made on the current thread while `body` runs.
fn allocations_during(body: impl FnOnce()) -> usize {
    let before = ALLOCATIONS.with(Cell::get);
    body();
    ALLOCATIONS.with(Cell::get) - before
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

#[test]
fn resolving_a_component_pointer_allocates_nothing() {
    let openapi = spec();

    let allocations = allocations_during(|| {
        for _ in 0..1_000 {
            let resolved = openapi.resolve_ref::<Schema>("#/components/schemas/A");
            assert!(resolved.is_ok());
        }
    });

    assert_eq!(allocations, 0, "a multi-hop component resolve allocated");
}

#[test]
fn resolving_an_escaped_pointer_allocates_to_decode_the_name() {
    // The documented exception. If this ever reaches zero the README claim can
    // be strengthened; until then it must stay qualified.
    let openapi = spec();

    let allocations = allocations_during(|| {
        let resolved = openapi.resolve_ref::<PathItem>("#/paths/~1pets");
        assert!(resolved.is_ok());
    });

    assert!(
        allocations > 0,
        "expected the escape to be decoded into a new name"
    );
}

#[test]
fn allocations_on_other_threads_are_not_counted() {
    // Another thread allocates while this one is measuring. A process-wide
    // counter attributes that to the measured body, which is how the test
    // harness's own allocations made the zero-allocation check flaky in CI.
    let start = std::sync::Barrier::new(2);
    let done = std::sync::Barrier::new(2);

    std::thread::scope(|scope| {
        scope.spawn(|| {
            start.wait();
            drop(std::hint::black_box(vec![0_u8; 64]));
            done.wait();
        });

        let allocations = allocations_during(|| {
            start.wait();
            done.wait();
        });

        assert_eq!(allocations, 0, "counted another thread's allocation");
    });
}
