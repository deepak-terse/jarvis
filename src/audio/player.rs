use super::resample;
use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Samples waiting to be played, shared between producers (TTS) and the audio callback.
pub struct AudioQueue {
    samples: Mutex<VecDeque<f32>>,
    rate: u32,
    first_audio: Mutex<Option<Instant>>,
}

impl AudioQueue {
    pub fn push(&self, samples: &[f32], rate: u32) {
        self.samples.lock().unwrap().extend(resample(samples, rate, self.rate));
    }

    pub fn is_empty(&self) -> bool {
        self.samples.lock().unwrap().is_empty()
    }

    /// When the first sample since the last `reset` reached the device.
    pub fn first_audio(&self) -> Option<Instant> {
        *self.first_audio.lock().unwrap()
    }

    pub fn reset(&self) {
        *self.first_audio.lock().unwrap() = None;
    }
}

/// Plays queued mono audio on the default output device.
pub struct Player {
    _stream: cpal::Stream,
    queue: Arc<AudioQueue>,
}

impl Player {
    pub fn new() -> Result<Self> {
        let device = cpal::default_host().default_output_device().context("no audio output device")?;
        let config = device.default_output_config()?;
        if config.sample_format() != cpal::SampleFormat::F32 {
            bail!("unsupported output sample format {:?}", config.sample_format());
        }
        let channels = config.channels() as usize;
        let queue = Arc::new(AudioQueue {
            samples: Mutex::new(VecDeque::new()),
            rate: config.sample_rate().0,
            first_audio: Mutex::new(None),
        });
        let q = queue.clone();
        let stream = device.build_output_stream(
            &config.into(),
            move |out: &mut [f32], _| {
                let mut samples = q.samples.lock().unwrap();
                let playing = !samples.is_empty();
                for frame in out.chunks_mut(channels) {
                    frame.fill(samples.pop_front().unwrap_or(0.0));
                }
                if playing {
                    q.first_audio.lock().unwrap().get_or_insert_with(Instant::now);
                }
            },
            |e| tracing::error!("output stream error: {e}"),
            None,
        )?;
        stream.play()?;
        Ok(Self { _stream: stream, queue })
    }

    pub fn queue(&self) -> Arc<AudioQueue> {
        self.queue.clone()
    }

    /// Wait until everything queued has been played.
    pub async fn drain(&self) {
        while !self.queue.is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tokio::time::sleep(Duration::from_millis(150)).await; // device buffer tail
    }
}
