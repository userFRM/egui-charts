//! The memory compiling a script holds, counted by an allocator of this test binary's own.

use egui_charts::studies::IndicatorFactory;
use egui_charts_scripting::Script;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The system allocator, counting the bytes allocated and the most allocated at once.
struct Counting;

static HELD: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every call goes to the system allocator unchanged; the counters only observe it.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let held = HELD.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(held, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        HELD.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// A host compiles a script when the user applies it, usually on the UI thread, so what it holds
/// meanwhile must follow the source: 22 KB that read a constant array of 9,999 items 1,000 times
/// stay under 16 MiB.
#[test]
fn compiling_holds_memory_in_proportion_to_the_source() {
    let source = format!(
        "const a = [{}];\n{}plot(\"x\", 1.0);",
        vec!["0"; 9_999].join(", "),
        "a;\n".repeat(1_000)
    );
    let factory = IndicatorFactory::new();
    let before = HELD.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let script = Script::compile("Test", &source, &factory);
    let peak = PEAK.load(Ordering::Relaxed) - before;
    assert!(script.is_ok(), "{script:?}");
    assert!(peak < 16 << 20, "compiling held {} KiB", peak >> 10);
}
