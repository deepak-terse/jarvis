//! TTS on its own thread, so synthesis never blocks the reply stream.

use crate::adapters::audio::AudioQueue;
use crate::capabilities::Warm;
use crate::core::contracts::capabilities::Tts;
use std::sync::{Arc, mpsc};
use tokio::sync::oneshot;

/// TTS worker: synthesizes sentences in order on its own thread and queues the audio.
pub struct Speaker {
    jobs: mpsc::Sender<Job>,
}

enum Job {
    Say(String, String),
    Flush(oneshot::Sender<()>),
}

impl Speaker {
    pub fn spawn(mut tts: Warm<Box<dyn Tts>>, queue: Arc<AudioQueue>) -> Self {
        let (jobs, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for job in rx {
                match job {
                    Job::Say(text, voice) => match tts.get().and_then(|t| t.synthesize(&text, &voice)) {
                        Ok(audio) => queue.push(&audio.samples, audio.rate),
                        Err(e) => eprintln!("\ntts error: {e:#}"),
                    },
                    Job::Flush(done) => {
                        tts.idle();
                        let _ = done.send(());
                    }
                }
            }
        });
        Self { jobs }
    }

    pub fn say(&self, text: String, voice: String) {
        let _ = self.jobs.send(Job::Say(text, voice));
    }

    /// Wait until every queued sentence has been synthesized.
    pub async fn flush(&self) {
        let (tx, rx) = oneshot::channel();
        if self.jobs.send(Job::Flush(tx)).is_ok() {
            let _ = rx.await;
        }
    }
}
