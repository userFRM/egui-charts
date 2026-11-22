//! Local notification tones shared by native playback and browser audio.

#[cfg(not(target_arch = "wasm32"))]
use maudio::{
    audio::sample_rate::SampleRate,
    data_source::sources::buffer::{AudioBuffer, AudioBufferBuilder},
    engine::{Engine, engine_builder::EngineBuilder},
    sound::{Sound, sound_builder::SoundBuilder, sound_flags::SoundFlags},
};

const SAMPLE_RATE: u32 = 44_100;

fn samples(name: &str, volume: f32) -> Vec<f32> {
    let frequencies: &[f32] = match name {
        "Bell" => &[880.0],
        "Beep" => &[440.0],
        _ => &[660.0, 880.0],
    };
    let volume = if volume.is_finite() {
        volume.clamp(0.0, 1.0)
    } else {
        0.0
    };
    (0..SAMPLE_RATE / 2)
        .map(|index| {
            let time = index as f32 / SAMPLE_RATE as f32;
            let note = (time * frequencies.len() as f32 / 0.5).floor() as usize;
            let local = time - note as f32 * 0.5 / frequencies.len() as f32;
            let envelope = (local * 200.0).min(1.0) * (-12.0 * local).exp();
            (std::f32::consts::TAU * frequencies[note.min(frequencies.len() - 1)] * local).sin()
                * envelope
                * volume
                * 0.25
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
pub(super) struct AudioPlayer {
    engine: Option<Engine>,
    // Each sound must be dropped before its source buffer.
    voices: Vec<(Sound, AudioBuffer<f32>)>,
}

#[cfg(not(target_arch = "wasm32"))]
impl AudioPlayer {
    pub(super) fn play(
        &mut self,
        name: &str,
        volume: f32,
        _activate: bool,
        _ctx: &egui::Context,
    ) -> Result<(), String> {
        if !volume.is_finite() || volume <= 0.0 {
            return Ok(());
        }
        if self.engine.is_none() {
            let mut builder = EngineBuilder::new();
            builder.set_sample_rate(SampleRate::Custom(SAMPLE_RATE));
            // Tests mix into an engine without a device: none plays through the machine's
            // speakers or depends on its audio hardware.
            #[cfg(test)]
            builder.no_device(1, SampleRate::Custom(SAMPLE_RATE));
            self.engine = Some(
                builder
                    .build()
                    .map_err(|error| format!("Could not open audio: {error}"))?,
            );
        }
        let engine = self.engine.as_ref().unwrap();
        self.voices.retain(|(sound, _)| sound.is_playing());
        if self.voices.len() >= 16 {
            self.voices.remove(0);
        }
        let pcm = samples(name, volume);
        let buffer = AudioBufferBuilder::build_f32(1, &pcm)
            .map_err(|error| format!("Could not load sound: {error}"))?;
        let sound = SoundBuilder::new(engine)
            .data_source(&buffer)
            .flags(SoundFlags::NO_PITCH | SoundFlags::NO_SPATIALIZATION)
            .build()
            .map_err(|error| format!("Could not load sound: {error}"))?;
        sound
            .play_sound()
            .map_err(|error| format!("Could not play sound: {error}"))?;
        self.voices.push((sound, buffer));
        Ok(())
    }

    pub(super) fn take_error(&mut self) -> Option<String> {
        None
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Default)]
pub(super) struct AudioPlayer {
    context: Option<web_sys::AudioContext>,
    error: std::rc::Rc<std::cell::RefCell<Option<String>>>,
}

#[cfg(target_arch = "wasm32")]
impl AudioPlayer {
    pub(super) fn play(
        &mut self,
        name: &str,
        volume: f32,
        activate: bool,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        if !volume.is_finite() || volume <= 0.0 {
            return Ok(());
        }
        let error_text = |error: wasm_bindgen::JsValue| {
            format!(
                "Could not play audio: {error:?}. Click Test sound in settings to enable browser audio."
            )
        };
        if self.context.is_none() {
            self.context = Some(web_sys::AudioContext::new().map_err(error_text)?);
        }
        let audio = self.context.as_ref().unwrap().clone();
        if !activate && audio.state() != web_sys::AudioContextState::Running {
            return Err("Click Test sound in settings to enable browser audio.".into());
        }
        let resume = audio.resume().map_err(error_text)?;
        let pcm = samples(name, volume);
        let error = self.error.clone();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = async {
                wasm_bindgen_futures::JsFuture::from(resume).await?;
                let buffer = audio.create_buffer(1, pcm.len() as u32, SAMPLE_RATE as f32)?;
                buffer.copy_to_channel(&pcm, 0)?;
                let source = audio.create_buffer_source()?;
                source.set_buffer(Some(&buffer));
                source.connect_with_audio_node(&audio.destination())?;
                source.start()?;
                Ok::<_, wasm_bindgen::JsValue>(())
            }
            .await;
            if let Err(value) = result {
                *error.borrow_mut() = Some(error_text(value));
                ctx.request_repaint();
            }
        });
        Ok(())
    }

    pub(super) fn take_error(&mut self) -> Option<String> {
        self.error.borrow_mut().take()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn muted_native_tones_do_not_open_a_device() {
        let mut player = AudioPlayer::default();
        let ctx = egui::Context::default();
        for volume in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            player.play("Bell", volume, false, &ctx).unwrap();
        }
        assert!(player.engine.is_none());
        assert!(player.voices.is_empty());
    }

    #[test]
    fn native_tones_mix_and_reclaim_bounded_voices() {
        let mut player = AudioPlayer {
            engine: Some(
                EngineBuilder::new()
                    .no_device(1, SampleRate::Custom(SAMPLE_RATE))
                    .build()
                    .unwrap(),
            ),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        for _ in 0..17 {
            player.play("Beep", 0.05, false, &ctx).unwrap();
        }
        assert_eq!(player.voices.len(), 16);

        let mut reader = player
            .engine
            .as_ref()
            .unwrap()
            .try_acquire_reader()
            .unwrap();
        let mut output = vec![0.0; SAMPLE_RATE as usize];
        assert_eq!(
            reader.read_pcm_frames_into(&mut output).unwrap(),
            output.len()
        );
        let expected = samples("Beep", 0.05);
        assert!(expected.iter().any(|sample| sample.abs() > 0.001));
        for (index, (actual, expected)) in output.iter().zip(&expected).enumerate() {
            assert!(
                (actual - expected * 16.0).abs() < 1e-5,
                "sample {index}: {actual}, expected {}",
                expected * 16.0
            );
        }
        assert!(output[expected.len()..].iter().all(|sample| *sample == 0.0));
        player.play("Bell", 0.5, false, &ctx).unwrap();
        assert_eq!(player.voices.len(), 1);
    }

    #[test]
    fn local_tones_have_distinct_finite_pcm_and_zero_volume_is_silent() {
        let mut outputs = Vec::new();
        for (name, volume) in [("Chime", 1.0), ("Bell", 1.0), ("Beep", 1.0), ("Bell", 0.0)] {
            let output = samples(name, volume);
            assert!(
                output
                    .iter()
                    .all(|sample| sample.is_finite() && sample.abs() <= 0.25)
            );
            assert_eq!(
                output.iter().any(|sample| sample.abs() > 0.001),
                volume > 0.0,
                "{name} at {volume}"
            );
            outputs.push(output);
        }
        assert_ne!(outputs[0], outputs[1]);
        assert_ne!(outputs[1], outputs[2]);
        // A 440 Hz sine reaches a quarter cycle at 1/1760 second. The sample at t=25/44100
        // is just short of that peak, multiplied by the attack and exponential decay.
        let pcm = samples("Beep", 1.0);
        let expected = (std::f64::consts::TAU * 440.0 * 25.0 / 44100.0).sin()
            * (200.0 * 25.0 / 44100.0)
            * (-12.0_f64 * 25.0 / 44100.0).exp()
            * 0.25;
        assert!((f64::from(pcm[25]) - expected).abs() < 1e-7);
        assert_eq!(samples("Beep", 0.5)[25], pcm[25] * 0.5);
    }

    /// Fills matching a time and sales size filter with sound play the alert sound chosen in the
    /// chart settings, at their volume, once however many fills of the frame match; a filter
    /// without sound plays nothing, and no notice stands in for the sound.
    #[test]
    fn time_and_sales_size_filters_with_sound_play_the_alert_sound_once() {
        use egui_charts::ui::widget_bar::TradeSizeFilter;
        let ctx = egui::Context::default();
        let mut app = super::super::DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            chrono::Utc::now(),
            true,
        );
        app.settings_state.alerts.sound = "Bell".into();
        app.settings_state.alerts.sound_volume = 0.35;
        let toasts = app.toasts.len();
        let fill = |size| egui_charts::model::Tick::new(chrono::Utc::now(), 100.0, size, None);
        for (sound, voices) in [(false, 0), (true, 1)] {
            let mut filter = TradeSizeFilter::new(5.0);
            filter.sound = sound;
            app.time_and_sales.filters = vec![filter];
            // Two fills of the frame reach the filter and one does not.
            app.feed_time_and_sales(&ctx, vec![fill(5.0), fill(1.0), fill(7.0)]);
            assert_eq!(app.audio.voices.len(), voices, "sound={sound}");
        }
        assert_eq!(app.toasts.len(), toasts);
        // With one voice, the mix is the Bell tone at the alert volume.
        let mut reader = app
            .audio
            .engine
            .as_ref()
            .unwrap()
            .try_acquire_reader()
            .unwrap();
        let mut output = vec![0.0; SAMPLE_RATE as usize];
        reader.read_pcm_frames_into(&mut output).unwrap();
        let expected = samples("Bell", 0.35);
        for (index, (actual, expected)) in output.iter().zip(&expected).enumerate() {
            assert!(
                (actual - expected).abs() < 1e-5,
                "sample {index}: {actual}, expected {expected}"
            );
        }
    }

    /// Order-flow detections whose rule asks for a sound play the alert sound chosen in the chart
    /// settings at the alert volume, as price alerts and time and sales filters do.
    #[test]
    fn order_flow_detections_with_sound_play_the_alert_sound() {
        let ctx = egui::Context::default();
        let now = "2026-11-04T17:30:00Z".parse().unwrap();
        let mut app = super::super::DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            now,
            true,
        );
        // The chart subscribes to its feed, which tracks the symbol, on its first frames.
        super::super::tests::settle(&mut app, &ctx);
        assert!(app.audio.voices.is_empty());
        app.settings_state.alerts.sound = "Bell".into();
        app.settings_state.alerts.sound_volume = 0.35;
        let detectors = app.live.detectors();
        let mut settings = detectors.settings();
        settings.big_trades.enabled = true;
        settings.big_trades.alert = true;
        settings.big_trades.sound = true;
        settings.big_trades.rule.threshold = egui_charts::services::Threshold::Fixed(10.0);
        detectors.set_settings(settings);
        let time = app.live.now();
        let fill = egui_charts::model::Tick::new(time, 100.0, 50.0, None);
        let event = egui_charts::data::MarketEvent::Trade(fill);
        detectors.push(app.chart.symbol(), time, &event);
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| eframe::App::ui(&mut *app, ui, &mut eframe::Frame::_new_kittest()),
        )
        .drop_without_applying_deltas();
        assert_eq!(app.audio.voices.len(), 1);
        let mut reader = app
            .audio
            .engine
            .as_ref()
            .unwrap()
            .try_acquire_reader()
            .unwrap();
        let mut output = vec![0.0; SAMPLE_RATE as usize];
        reader.read_pcm_frames_into(&mut output).unwrap();
        let expected = samples("Bell", 0.35);
        for (index, (actual, expected)) in output.iter().zip(&expected).enumerate() {
            assert!(
                (actual - expected).abs() < 1e-5,
                "sample {index}: {actual}, expected {expected}"
            );
        }
    }
}
