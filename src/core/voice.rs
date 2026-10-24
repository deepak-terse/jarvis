//! push-to-talk → stt → agent → sentences → clean → tts → playback.

use super::agent::{Agent, Console, Sink};
use super::session::Session;
use crate::audio::{self, AudioQueue, Player, Recording};
use crate::capabilities::{
    Warm, stt,
    tts::{self, Tts},
};
use crate::config::Config;
use crate::telemetry::ms_since;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags};
use crossterm::{execute, terminal};
use regex::Regex;
use serde_json::Value;
use std::sync::{Arc, LazyLock, mpsc};
use std::time::Instant;
use tokio::sync::oneshot;

pub async fn run(
    agent: &Agent,
    session: &mut Session,
    cfg: &Config,
    mut greeting: Option<oneshot::Receiver<String>>,
) -> Result<()> {
    eprintln!("Loading speech models…");
    let mut stt = stt::create(cfg.capability("stt")?)?;
    let player = Player::new()?;
    let speaker = Speaker::spawn(tts::create(cfg.capability("tts")?)?, player.queue());
    let hold = terminal::supports_keyboard_enhancement().unwrap_or(false);
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

    while let Some((pcm, rate)) = push_to_talk(hold)? {
        let t0 = Instant::now(); // end of speech
        let pcm = audio::resample(&pcm, rate, stt::SAMPLE_RATE);
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

/// Wait for SPACE, record until it is released (or pressed again on terminals that
/// cannot report key releases). Returns `None` when the user quits.
fn push_to_talk(hold: bool) -> Result<Option<(Vec<f32>, u32)>> {
    struct Raw(bool);
    impl Drop for Raw {
        fn drop(&mut self) {
            if self.0 {
                let _ = execute!(std::io::stdout(), event::PopKeyboardEnhancementFlags);
            }
            let _ = terminal::disable_raw_mode();
        }
    }
    terminal::enable_raw_mode()?;
    if hold {
        execute!(std::io::stdout(), event::PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::REPORT_EVENT_TYPES))?;
    }
    let _raw = Raw(hold);

    loop {
        if let Event::Key(k) = event::read()? {
            match k.code {
                KeyCode::Char(' ') if k.kind == KeyEventKind::Press => break,
                KeyCode::Char('q') | KeyCode::Esc => return Ok(None),
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => return Ok(None),
                _ => {}
            }
        }
    }
    let recording = Recording::start()?;
    print!("\r\x1b[2K● listening…");
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let stop_on = if hold { KeyEventKind::Release } else { KeyEventKind::Press };
    loop {
        if let Event::Key(k) = event::read()?
            && k.code == KeyCode::Char(' ')
            && k.kind == stop_on
        {
            break;
        }
    }
    print!("\r\x1b[2K");
    Ok(Some(recording.stop()))
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

/// Byte index just past the first speakable sentence in `buf`, if complete.
/// Boundaries: newline, or `.!?;:` followed by whitespace. Very long runs split at a comma/space.
fn sentence_end(buf: &str) -> Option<usize> {
    let mut chars = buf.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let boundary = c == '\n' || (".!?;:".contains(c) && chars.peek().is_some_and(|(_, n)| n.is_whitespace()));
        if boundary && buf[..i].trim().len() >= 2 {
            return Some(i + c.len_utf8());
        }
    }
    if buf.len() > 200 {
        return buf.rfind(", ").or_else(|| buf.rfind(' ')).map(|i| i + 1);
    }
    None
}

/// Make model text speakable: drop markdown, citations and URLs.
pub fn clean(text: &str) -> String {
    static RULES: LazyLock<[(Regex, &str); 6]> = LazyLock::new(|| {
        [
            (Regex::new(r"\[([^\]]+)\]\([^)]*\)").unwrap(), "$1"), // [text](url) → text
            (Regex::new(r"\[\d+(?:[,\s]*\d+)*\]").unwrap(), ""),   // [1], [2, 3]
            (Regex::new(r"https?://\S+").unwrap(), ""),            // bare URLs
            (Regex::new(r"(?m)^\s*(?:[-*•]|\d+\.)\s+|[*_`#>|~]").unwrap(), ""), // bullets, emphasis, headings
            (Regex::new(r"\s+").unwrap(), " "),
            (Regex::new(r" ([.,!?;:])").unwrap(), "$1"), // space left before punctuation
        ]
    });
    RULES.iter().fold(text.to_string(), |s, (re, rep)| re.replace_all(&s, *rep).into_owned()).trim().to_string()
}

/// TTS worker: synthesizes sentences in order on its own thread and queues the audio.
struct Speaker {
    jobs: mpsc::Sender<Job>,
}

enum Job {
    Say(String, String),
    Flush(oneshot::Sender<()>),
}

impl Speaker {
    fn spawn(mut tts: Warm<Box<dyn Tts>>, queue: Arc<AudioQueue>) -> Self {
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

    fn say(&self, text: String, voice: String) {
        let _ = self.jobs.send(Job::Say(text, voice));
    }

    /// Wait until every queued sentence has been synthesized.
    async fn flush(&self) {
        let (tx, rx) = oneshot::channel();
        if self.jobs.send(Job::Flush(tx)).is_ok() {
            let _ = rx.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_sentences_but_not_decimals() {
        let buf = "It is 28.5 degrees. Humid too";
        let end = sentence_end(buf).unwrap();
        assert_eq!(&buf[..end], "It is 28.5 degrees.");
        assert_eq!(sentence_end("Humid too"), None);
    }

    #[test]
    fn cleans_markdown_citations_urls() {
        let s = "**Sunny** in [Mumbai](https://x.com) today [1][2]. See https://weather.com\n- High: 31°C";
        assert_eq!(clean(s), "Sunny in Mumbai today. See High: 31°C");
    }
}

/// Phase 0 smoke test: LLM, STT and TTS loaded and running in one process.
/// `cargo test --release -- --ignored` (needs the models from config.toml).
#[cfg(test)]
mod smoke {
    use crate::capabilities::{stt, tts};
    use crate::config::{Config, expand};
    use crate::llm::{self, ChatRequest, LlmEvent, Message, Prefix, Role};

    #[tokio::test]
    #[ignore]
    async fn llm_stt_tts_in_one_process() {
        let (cfg, _) = Config::load("config.toml".as_ref()).unwrap();
        let llm = llm::create(&cfg.llm).unwrap();
        let mut stt = stt::create(cfg.capability("stt").unwrap()).unwrap();
        let mut tts = tts::create(cfg.capability("tts").unwrap()).unwrap();

        let audio = tts.get().unwrap().synthesize("Hello there, how is the weather today?", "af_bella").unwrap();
        let pcm = crate::audio::resample(&audio.samples, audio.rate, stt::SAMPLE_RATE);
        let heard = stt.get().unwrap().transcribe(&pcm).unwrap().to_lowercase();
        assert!(heard.contains("weather"), "round-trip transcript: {heard}");

        let wav_dir = cfg.capability("stt").unwrap().settings["model"].as_str().unwrap().to_string();
        let wav = sherpa_onnx::Wave::read(&format!("{}/test_wavs/0.wav", expand(wav_dir.as_ref()).display())).unwrap();
        println!("whisper: {}", stt.get().unwrap().transcribe(wav.samples()).unwrap());

        let prefix = Prefix { cache_key: "t".into(), system: "Be brief.".into(), tools: vec![] };
        let req = ChatRequest { prefix, messages: vec![Message::new(Role::User, heard)], temperature: 0.0 };
        let mut rx = llm.chat(req);
        while let Some(ev) = rx.recv().await {
            if let LlmEvent::Done(c) = ev {
                assert!(!c.text.is_empty());
                println!("llm: {}", c.text);
                break;
            }
            assert!(!matches!(ev, LlmEvent::Error(_)), "{ev:?}");
        }
    }
}
