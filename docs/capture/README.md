# Demo media

On Linux, from the repository root:

```sh
docs/capture/regenerate.sh /tmp/egui-charts-media
```

Use a fresh directory outside the checkout. The command builds the standalone capture executable, records each scene twice, compares every PNG byte for byte, encodes the motion media and poster and checks their dimensions, durations, frame rates and size budgets. Set `PYTHON` if the interpreter with Pillow is not the default `python3`. It leaves the PNGs in the temporary directory. It requires the stable Rust toolchain, a C compiler, a Vulkan software renderer, Python with Pillow, and the encoders used in `regenerate.sh`. No library dependency is added.

The executable compiles the same `DemoApp` as `examples/demo`. Initialization uses its controller actions and public chart API. Recorded actions use pointer and keyboard events against the actual controls. `animation.rs` scripts each scene in milliseconds and logical points. The drawing step selects the real tool, checks its anchors against the bars and verifies a handle move. The synthetic feed starts at 2026-10-27 19:29:50 UTC, paused, and advances explicitly. The tour and order-flow scene warm 25 minutes of book history; all four tour feeds reach the same minute before recording begins at 19:54:50 UTC. UI time advances by exactly 1/60 second per rendered frame. Motion is always forward. A final 0.35 second dissolve meets the opening view without reversing any frames. Replay starts with a click on a bar, as in the demo; the tour keeps the replaying chart in its own synchronization group, so the other three charts keep their history and depth.

`clock.c` fixes the capture process's real-time clock and advances the main thread's monotonic clock during each UI pass. This lets the existing replay transport advance historical bars on the scripted timeline; an assertion verifies playback before pausing it. The frame writer restores the real monotonic clock before GPU rendering, and background threads always retain their normal deadlines. The shim is compiled into the temporary directory and loaded only by the recording processes. `LP_NUM_THREADS=3` fixes the software renderer's worker count. Repeatability assumes the same renderer, fonts, executable and encoder versions.

The tour records 1280×720 logical points at 2 pixels per point, producing 2560×1440 PNGs at 60 fps. Gallery scenes use 1280×800 logical points at the same density, with larger axis text to remain readable in README columns; they encode to 1280×800 pixels. Every frame is rendered. A small capture-only app delegates to the demo and paints an original vector pointer over its output, following the cursor icon selected by the real controls.

`regenerate.sh` contains the exact encoding commands. The recorded encoder versions are FFmpeg 6.1.1 and libwebp 1.6.0 (`cwebp` and `img2webp`). Animated WebP uses alternating 33, 33 and 34 millisecond frame delays, exactly 30 frames per second before identical holds are coalesced. The site receives H.264 High and AV1 video at 1920×1080, 60 fps, with 4:2:0 pixels; the MP4 stores its metadata before its media data. The poster retains 2560×1440 pixels. The README receives separate dark and light recordings at 1280×720, 30 fps, and 1280×800 gallery loops at 30 fps. The light recording runs the same actions with the demo's light theme.

The other README images are stills. Every scene waits for the active chart's history after each step, so each pane of a layout ends at the same sample time, and the layout stills assert it. They are converted to lossless WebP with the command in the header of `main.rs`:

```sh
cargo run --release --manifest-path docs/capture/Cargo.toml -- /tmp/egui-charts-stills
```
