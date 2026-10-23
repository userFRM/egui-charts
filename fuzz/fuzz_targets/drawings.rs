//! Drawing files: the saved formats the crate loads, then the drawings restored and saved again.

#![no_main]

use egui_charts::drawings::persistence::{DrawingStorage, stored_to_drawing};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let json = String::from_utf8_lossy(data);
    let Ok((storage, _skipped)) = DrawingStorage::from_json(&json) else {
        return;
    };
    for stored in storage.drawings() {
        let mut drawing = stored_to_drawing(stored, |ms| ms as f32, 0);
        drawing.update_screen_coords(|bar| bar * 8.0, |price| price as f32);
    }
    if let Ok(saved) = storage.to_json() {
        let _ = DrawingStorage::from_json(&saved);
    }
});
