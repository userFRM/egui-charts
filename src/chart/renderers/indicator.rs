pub use crate::chart::indicators::render::IndicatorRenderer;

#[cfg(all(test, feature = "studies"))]
mod tests {
    use crate::studies::{BollingerBands, Indicator, SMA, StandardErrorBands};
    use crate::widget::test_frames::{chart, shapes};
    use egui::{Color32, Context, Shape};

    #[test]
    fn overlay_lines_use_one_point_strokes_and_study_colours() {
        let blue = Color32::from_rgb(41, 98, 255);
        let orange = Color32::from_rgb(255, 167, 38);
        let custom = Color32::from_rgb(10, 200, 90);
        let mut recoloured = BollingerBands::default();
        recoloured.set_colors(vec![custom]);
        let mut sma = SMA::default();
        sma.set_colors(vec![custom]);
        let cases: [(Box<dyn Indicator>, Vec<Color32>); 4] = [
            (
                Box::new(BollingerBands::default()),
                vec![blue, orange, blue],
            ),
            (Box::new(sma), vec![custom]),
            (Box::new(recoloured), vec![custom; 3]),
            (
                Box::new(StandardErrorBands::new(20, 2.0)),
                vec![
                    Color32::from_rgb(156, 39, 176),
                    Color32::from_rgb(120, 123, 134),
                ],
            ),
        ];
        for (study, colours) in cases {
            let name = study.name().to_owned();
            let mut chart = chart(&[100.0], &[110.0]);
            chart.add_indicator(study);
            let ctx = Context::default();
            let strokes: Vec<_> = shapes(&ctx, &mut chart, 0.0, Vec::new())
                .into_iter()
                .filter_map(|shape| match shape {
                    Shape::LineSegment { stroke, .. } if colours.contains(&stroke.color) => {
                        Some(stroke)
                    }
                    _ => None,
                })
                .collect();
            assert!(!strokes.is_empty(), "{name}: no overlay lines");
            assert!(
                strokes.iter().all(|stroke| stroke.width == 1.0),
                "{name}: {strokes:?}"
            );
            for colour in colours {
                assert!(
                    strokes.iter().any(|stroke| stroke.color == colour),
                    "{name}: missing {colour:?}"
                );
            }
        }
    }
}
