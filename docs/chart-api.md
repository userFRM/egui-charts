# Chart host contracts

Call `Chart::show` each frame and drain `Chart::drain_events` afterwards. A `ChartPointer` carries the rendered logical index, UTC time, UI position, pane, raw pane value, main bar, previous close, all added series with a matching timestamp (including hidden series), visible study values, hit target, modifiers and input device. Bars retain host JSON payloads. Missing timestamps have no added-series entry; main-series whitespace retains its slot. Price-based chart indices count displayed elements.

The host input integration can call `set_pointer_kind(Some(PointerKind::Pen))` before showing the chart. Keep it updated when the active device changes; `None` restores egui mouse/touch detection. egui itself cannot distinguish pens. Programmatic synchronized crosshairs do not emit local pointer moves. Click and double-click payloads use the same contract. The demo's event-only legend and tooltip integration belongs to the demo lane.
