use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

/// An in-progress mic capture (default input device, downmixed to mono).
pub struct Recording {
    _stream: cpal::Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    rate: u32,
}

impl Recording {
    pub fn start() -> Result<Self> {
        let device = cpal::default_host().default_input_device().context("no microphone found")?;
        let config = device.default_input_config()?;
        if config.sample_format() != cpal::SampleFormat::F32 {
            bail!("unsupported mic sample format {:?}", config.sample_format());
        }
        let channels = config.channels() as usize;
        let rate = config.sample_rate().0;
        let samples = Arc::new(Mutex::new(Vec::new()));
        let sink = samples.clone();
        let stream = device.build_input_stream(
            &config.into(),
            move |data: &[f32], _| {
                let mono = data.chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32);
                sink.lock().unwrap().extend(mono);
            },
            |e| tracing::error!("mic stream error: {e}"),
            None,
        )?;
        stream.play()?;
        Ok(Self { _stream: stream, samples, rate })
    }

    /// Stop capturing; returns (mono samples, sample rate).
    pub fn stop(self) -> (Vec<f32>, u32) {
        let rate = self.rate;
        drop(self._stream);
        let samples = std::mem::take(&mut *self.samples.lock().unwrap());
        (samples, rate)
    }
}
