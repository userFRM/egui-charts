//! Local notification tones shared by native playback and browser audio.

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
    engine: Option<soloud::Soloud>,
    voices: Vec<(soloud::Handle, soloud::audio::Wav)>,
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
        use soloud::AudioExt;
        if !volume.is_finite() || volume <= 0.0 {
            return Ok(());
        }
        if self.engine.is_none() {
            self.engine = Some(
                soloud::Soloud::default()
                    .map_err(|error| format!("Could not open audio: {error}"))?,
            );
        }
        let engine = self.engine.as_ref().unwrap();
        self.voices
            .retain(|(handle, _)| engine.is_valid_voice_handle(*handle));
        if self.voices.len() >= 16 {
            engine.stop(self.voices.remove(0).0);
        }
        let mut sound = soloud::audio::Wav::default();
        let pcm = samples(name, volume);
        // SAFETY: finite mono samples fit u32; the engine copies them and owns no Rust allocation.
        unsafe { sound.load_raw_wav_ex(&pcm, SAMPLE_RATE as f32, 1, true, false) }
            .map_err(|error| format!("Could not load sound: {error}"))?;
        self.voices.push((engine.play(&sound), sound));
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
}
