//! push-to-talk → stt → agent → sentences → clean → tts → playback.

mod push_to_talk;
mod speaker;
mod text;

use super::terminal::Console;
use crate::adapters::audio::{self, Player};
use crate::capabilities::{stt, tts};
use crate::core::agent::{Agent, Session};
use crate::core::contracts::capabilities::SAMPLE_RATE;
use crate::core::contracts::events::{Sink, ms_since};
use crate::core::contracts::settings::ProviderConfig;
use anyhow::Result;
use serde_json::Value;
use speaker::Speaker;
use std::time::Instant;
use text::{clean, sentence_end};
use tokio::sync::oneshot;

pub async fn run(
    agent: &Agent,
    session: &mut Session,
    stt_cfg: &ProviderConfig,
    tts_cfg: &ProviderConfig,
    mut greeting: Option<oneshot::Receiver<String>>,
) -> Result<()> {
    eprintln!("Loading speech models…");
    let mut stt = stt::create(stt_cfg)?;
    let player = Player::new()?;
    let speaker = Speaker::spawn(tts::create(tts_cfg)?, player.queue());
    let hold = push_to_talk::hold_supported();
    println!(
        "JARVIS voice mode — {} SPACE to talk, q to quit. Say \"switch to <persona>\" to change persona.",
        if hold { "hold" } else { "press" }
    );
    // Spoken only if it was ready by the time the speech models loaded.
    if let Some(text) = greeting.as_mut().and_then(|g| g.try_recv().ok()) {
        println!("{}> {text}", agent.persona(session).name);
        speaker.say(clean(&text), agent.persona(session).voice.clone());
        speaker.flush().await;
        player.drain().await;
    }

    while let Some((pcm, rate)) = push_to_talk::record(hold)? {
        let t0 = Instant::now(); // end of speech
        let pcm = audio::resample(&pcm, rate, SAMPLE_RATE);
        let text = stt.get()?.transcribe(&pcm)?;
        stt.idle();
        let stt_ms = ms_since(t0);
        if text.is_empty() {
            println!("(heard nothing)");
            continue;
        }
        println!("you> {text}");

        let queue = player.queue();
        queue.reset();
        print!("{}> ", agent.persona(session).name);
        let persona = agent.persona(session);
        let mut sink = VoiceSink {
            speaker: &speaker,
            voice: persona.voice.clone(),
            fillers: persona.fillers.clone(),
            buf: String::new(),
            first_sentence: None,
            filler_at: None,
        };
        let result = agent.respond(session, &text, "voice", t0, &mut sink).await;
        let (first_sentence, filler_at) = (sink.first_sentence, sink.filler_at);
        speaker.flush().await;
        player.drain().await;

        match result {
            Ok(Some(mut m)) => {
                let first_audio = queue.first_audio();
                m.stt_ms = Some(stt_ms);
                m.latency_ms = first_audio.map(|t| (t - t0).as_secs_f64() * 1000.0);
                m.tts_first_audio_ms = first_audio.zip(first_sentence).map(|(a, s)| (a - s).as_secs_f64() * 1000.0);
                m.filler_ms = filler_at.map(|f| (f - t0).as_secs_f64() * 1000.0);
                println!("\x1b[2m  ⏱ {} · stt {stt_ms:.0} ms\x1b[0m", m.summary());
                agent.log_metrics(&m);
            }
            Ok(None) => {}
            Err(e) => eprintln!("error: {e:#}"),
        }
    }
    Ok(())
}

/// Streams reply text to the terminal and, sentence by sentence, to TTS.
struct VoiceSink<'a> {
    speaker: &'a Speaker,
    voice: String,
    /// Phrases to say while a tool runs, so the user isn't left in silence.
    fillers: Vec<String>,
    buf: String,
    first_sentence: Option<Instant>,
    filler_at: Option<Instant>,
}

impl VoiceSink<'_> {
    fn say(&mut self, sentence: &str) {
        let text = clean(sentence);
        if !text.is_empty() {
            self.first_sentence.get_or_insert_with(Instant::now);
            self.speaker.say(text, self.voice.clone());
        }
    }
}

impl Sink for VoiceSink<'_> {
    fn text(&mut self, delta: &str) {
        Console.text(delta);
        self.buf.push_str(delta);
        while let Some(end) = sentence_end(&self.buf) {
            let sentence: String = self.buf.drain(..end).collect();
            self.say(&sentence);
        }
    }
    fn tool(&mut self, name: &str, args: &Value) {
        Console.tool(name, args);
        // Once per turn, and only if nothing has been said yet.
        if self.first_sentence.is_none() && self.buf.trim().is_empty() && !self.fillers.is_empty() {
            let pick =
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
            let filler = self.fillers[pick as usize % self.fillers.len()].clone();
            self.filler_at = Some(Instant::now());
            self.say(&filler);
        }
    }
    fn end(&mut self) {
        let rest = std::mem::take(&mut self.buf);
        self.say(&rest);
        Console.end();
    }
}
