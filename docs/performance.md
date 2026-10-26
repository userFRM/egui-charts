# Chart performance

Measure the application's whole frame, including tessellation and releasing the rendered meshes. `cargo bench --bench frame_time` does this without a window or GPU. It covers large histories, multiple charts, studies and drawings; `FRAME_TIME_FRAMES` sets the measured frames after 60 warm-up frames, and arguments select scenario names:

```sh
cargo bench --bench frame_time -- "zoomed out" "8 charts"
```

Keep the viewport size, physical pixel scale, visible range, features and studies the same when comparing builds. Initial history and study calculations are separate from steady frames. Alternate the two binaries across several rounds on the same machine, as described in [CONTRIBUTING.md](../CONTRIBUTING.md#measuring-frame-time).

## Linux allocation policy

With glibc's default allocator, large temporary mesh buffers can repeatedly extend the heap, be returned to the operating system when freed, and fault their pages back in on the next frame. These are the meshes returned by `egui::Context::tessellate`, owned by the host renderer. Reusing chart buffers does not retain them.

egui 0.37 adds a post-tessellation transform to `ClippedShape`. On x86-64 this grows each record from 80 to 96 bytes, changing the allocation sizes even for identical chart geometry. In an isolated experiment on the original workload, restoring 80-byte records for identity transforms removed steady-frame heap shrinks while preserving the output mesh sizes. Retaining the full transform behaviour with compact records requires an upstream change.

An application seeing this pattern can set both allocation thresholds before starting:

```sh
GLIBC_TUNABLES=glibc.malloc.mmap_threshold=67108864:glibc.malloc.trim_threshold=268435456 ./your-app
```

The corresponding startup `mallopt` parameters are `M_MMAP_THRESHOLD` at 64 MiB and `M_TRIM_THRESHOLD` at 256 MiB. Set both: configuring these parameters disables glibc's dynamic threshold adjustment. Raising only the trim threshold can leave large buffers allocated through fresh memory mappings, while raising only the mapping threshold can still leave the heap shrinking each frame.

An alternative measured setting is extra retained space at the top of the heap:

```sh
GLIBC_TUNABLES=glibc.malloc.top_pad=16777216 ./your-app
```

This corresponds to `M_TOP_PAD` at 16 MiB. Another alternative is an application-selected global allocator. All of these choices affect the whole process and can retain more memory, so compare frame times, page faults and memory use with the application's real workload. They apply to native glibc applications, not to browser builds or every Linux allocator. The library sets none of them.

In the original Linux measurements of [#472](https://github.com/userFRM/egui-charts/issues/472), with egui 0.37 and 3,506 visible bars from a history of 5,000,000, the frame took about 5.8 ms with defaults and 3.9 ms with both thresholds set. The latter was within 2% of the egui 0.33 frame. The 16 MiB top padding also removed the repeated page faults. These are measurements of that workload before pixel merging was added, not a promise about current frame times.

The [glibc allocation tunables](https://sourceware.org/glibc/manual/latest/html_node/Memory-Allocation-Tunables.html) and [startup parameters](https://sourceware.org/glibc/manual/latest/html_node/Malloc-Tunable-Parameters.html) describe the allocator's behaviour.

## Visible work

Time-based price series already merge ordinary bars sharing a physical pixel. This reduces geometry while retaining the group's price extremes; gaps and per-bar styles retain their source bars. Study lines draw each uninterrupted solid run as one joined path, preserving gaps, plot colours and stroke widths. Directional colours and gradient fills keep their segment rendering.

A settled chart without a visible countdown or animation requests no idle repaint. A drawn countdown with a feed or server time requests a repaint once a second; active animations request their next frame. New feed data wakes the chart. A browser runner can still schedule animation callbacks while no chart frame is due, so distinguish the runner's idle callbacks from actual chart passes when measuring idle main-thread time.
