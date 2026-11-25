//! What a script draws besides its plot values: the colours of the plots coloured by an
//! expression, as a row holds them, and the labels, lines, bands and markers its bars make.

use ecolor::Color32;
use egui_charts::studies::{IndicatorMarker, StudyLabel, StudySegment, StudyZone};

/// What one call of a drawing helper made.
#[derive(Clone, Debug)]
pub(crate) enum Drawn {
    Label(StudyLabel),
    Segment(StudySegment),
    Zone(StudyZone),
    Marker(IndicatorMarker),
}

impl Drawn {
    /// The bytes the study keeps for it, among the drawings made and in its list, its text
    /// included, for the memory limit.
    pub fn bytes(&self) -> usize {
        let text = match self {
            Self::Label(label) => label.text.capacity(),
            _ => 0,
        };
        2 * (std::mem::size_of::<(usize, Self)>() + text)
    }
}

/// The drawings of a study's bars, in the order the bars made them, and the lists the chart reads.
#[derive(Clone, Debug, Default)]
pub(crate) struct Drawings {
    /// Each drawing with the bar that made it, in bar order.
    made: Vec<(usize, Drawn)>,
    labels: Vec<StudyLabel>,
    segments: Vec<StudySegment>,
    zones: Vec<StudyZone>,
    markers: Vec<IndicatorMarker>,
}

impl Drawings {
    /// Keep the drawings made before bar `start`, add `made`, made from `start` on in bar order,
    /// and list each kind again, the labels in order of their bar and else as they were made, as
    /// the chart reads them.
    pub fn replace(&mut self, start: usize, made: Vec<(usize, Drawn)>) {
        let kept = self.made.partition_point(|(bar, _)| *bar < start);
        self.made.truncate(kept);
        self.made.extend(made);
        // shortcut: lists every drawing again on each run, O(n) plus an almost sorted sort of the
        // labels; keep the lists incrementally if frame profiles show it.
        self.labels.clear();
        self.segments.clear();
        self.zones.clear();
        self.markers.clear();
        for (_, drawn) in &self.made {
            match drawn {
                Drawn::Label(label) => self.labels.push(label.clone()),
                Drawn::Segment(segment) => self.segments.push(*segment),
                Drawn::Zone(zone) => self.zones.push(zone.clone()),
                Drawn::Marker(marker) => self.markers.push(*marker),
            }
        }
        self.labels.sort_by_key(|label| label.bar);
    }

    pub fn labels(&self) -> &[StudyLabel] {
        &self.labels
    }

    pub fn segments(&self) -> &[StudySegment] {
        &self.segments
    }

    pub fn zones(&self) -> &[StudyZone] {
        &self.zones
    }

    pub fn markers(&self) -> &[IndicatorMarker] {
        &self.markers
    }
}

/// A colour as a row holds it: its premultiplied bytes as an exact integer, NaN for none.
pub(crate) fn encode(color: Option<Color32>) -> f64 {
    color.map_or(f64::NAN, |color| {
        f64::from(u32::from_le_bytes(color.to_array()))
    })
}

/// The colour a row holds, `None` for NaN.
pub(crate) fn decode(value: f64) -> Option<Color32> {
    if value.is_nan() {
        return None;
    }
    let [r, g, b, a] = (value as u32).to_le_bytes();
    Some(Color32::from_rgba_premultiplied(r, g, b, a))
}
