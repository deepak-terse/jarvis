//! Embedded llama.cpp. The model and context live on a dedicated thread (an actor);
//! callers send jobs and receive streamed events over channels.

use super::LlmConfig;
use super::cache::PrefixCache;
use super::template::{self, Template};
use super::tools::StreamFilter;
use crate::core::contracts::events::ms_since;
use crate::core::contracts::model::{ChatRequest, Completion, GenStats, Llm, LlmEvent, Prefix};
use crate::core::contracts::tools::ToolCall;
use anyhow::{Context, Result, anyhow, bail};
use llama_cpp_2::LlamaStateSeqFlags;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use std::num::NonZeroU32;
use std::sync::{Mutex, mpsc};
use std::time::Instant;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

enum Job {
    Chat(ChatRequest, UnboundedSender<LlmEvent>),
    Warm(Prefix),
}

pub struct LlamaEngine {
    jobs: Option<mpsc::Sender<Job>>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// A second template instance for formatting on the caller's thread (stateless, so this is cheap).
    template: Mutex<Box<dyn Template>>,
}

impl LlamaEngine {
    /// Start the engine thread. With `keep_warm` the model is loaded before this returns,
    /// so configuration errors surface at startup.
    pub fn spawn(cfg: &LlmConfig) -> Result<Self> {
        let template = template::create(&cfg.template)?;
        let cfg_template = cfg.template.clone();
        let (jobs, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let cfg = cfg.clone();
        let thread = std::thread::Builder::new().name("llm".into()).spawn(move || {
            if let Err(e) = run(&cfg, &*template, rx, &ready_tx) {
                let _ = ready_tx.send(Err(e));
            }
        })?;
        ready_rx.recv().context("LLM thread exited")??;
        let template = Mutex::new(template::create(&cfg_template)?);
        Ok(Self { jobs: Some(jobs), thread: Some(thread), template })
    }
}

/// Free the model on its own thread before process exit; Metal asserts if it is still alive.
impl Drop for LlamaEngine {
    fn drop(&mut self) {
        self.jobs.take();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Llm for LlamaEngine {
    fn chat(&self, req: ChatRequest) -> UnboundedReceiver<LlmEvent> {
        let (tx, rx) = unbounded_channel();
        if let Some(Err(mpsc::SendError(Job::Chat(_, tx)))) = self.jobs.as_ref().map(|j| j.send(Job::Chat(req, tx))) {
            let _ = tx.send(LlmEvent::Error("LLM engine stopped".into()));
        }
        rx
    }

    fn warm(&self, prefix: Prefix) {
        if let Some(j) = &self.jobs {
            let _ = j.send(Job::Warm(prefix));
        }
    }

    fn format_tool_call(&self, call: &ToolCall) -> String {
        self.template.lock().unwrap().tool_call(call)
    }
}

fn run(
    cfg: &LlmConfig,
    template: &dyn Template,
    jobs: mpsc::Receiver<Job>,
    ready: &mpsc::SyncSender<Result<()>>,
) -> Result<()> {
    llama_cpp_2::send_logs_to_tracing(llama_cpp_2::LogOptions::default());
    let backend = LlamaBackend::init()?;
    let load = || {
        LlamaModel::load_from_file(&backend, &cfg.model, &LlamaModelParams::default().with_n_gpu_layers(cfg.gpu_layers))
            .map_err(|e| anyhow!("loading {}: {e}", cfg.model.display()))
    };

    if cfg.keep_warm {
        let model = load()?;
        let mut engine = Engine::new(&backend, &model, cfg, template)?;
        let _ = ready.send(Ok(()));
        for job in jobs {
            engine.handle(job);
        }
    } else {
        // Cold mode: load per job and release memory afterwards. Warm-up jobs are pointless here.
        let _ = ready.send(Ok(()));
        for job in jobs {
            let Job::Chat(req, out) = job else { continue };
            match load().and_then(|model| {
                Engine::new(&backend, &model, cfg, template)?.handle(Job::Chat(req, out.clone()));
                Ok(())
            }) {
                Ok(()) => {}
                Err(e) => {
                    let _ = out.send(LlmEvent::Error(e.to_string()));
                }
            }
        }
    }
    Ok(())
}

struct Engine<'m> {
    model: &'m LlamaModel,
    ctx: LlamaContext<'m>,
    template: &'m dyn Template,
    /// Tokens currently resident in the KV cache (sequence 0).
    kv: Vec<LlamaToken>,
    cache: PrefixCache,
    max_tokens: u32,
}

impl<'m> Engine<'m> {
    fn new(backend: &LlamaBackend, model: &'m LlamaModel, cfg: &LlmConfig, template: &'m dyn Template) -> Result<Self> {
        let flash = if cfg.flash_attn {
            llama_cpp_sys_2::LLAMA_FLASH_ATTN_TYPE_ENABLED
        } else {
            llama_cpp_sys_2::LLAMA_FLASH_ATTN_TYPE_DISABLED
        };
        let params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(cfg.context_size))
            .with_n_batch(512)
            .with_flash_attention_policy(flash);
        let ctx = model.new_context(backend, params)?;
        Ok(Self { model, ctx, template, kv: Vec::new(), cache: PrefixCache::default(), max_tokens: cfg.max_tokens })
    }

    fn handle(&mut self, job: Job) {
        match job {
            Job::Warm(prefix) => {
                let tokens = self.tokenize_prefix(&prefix);
                if let Err(e) = self.ensure_prefix(&prefix.cache_key, &tokens) {
                    tracing::warn!("prefix warm-up failed: {e}");
                }
            }
            Job::Chat(req, out) => {
                if let Err(e) = self.chat(&req, &out) {
                    self.kv.clear(); // KV state is unknown after a failure; force a rebuild
                    self.ctx.clear_kv_cache();
                    let _ = out.send(LlmEvent::Error(e.to_string()));
                }
            }
        }
    }

    fn tokenize_prefix(&self, prefix: &Prefix) -> Vec<LlamaToken> {
        let vocab = self.model.vocab();
        let text = self.template.prefix(&prefix.system, &prefix.tools);
        vocab.tokenize(text.as_bytes(), vocab.should_add_bos(), true)
    }

    fn chat(&mut self, req: &ChatRequest, out: &UnboundedSender<LlmEvent>) -> Result<()> {
        let started = Instant::now();
        let prefix = self.tokenize_prefix(&req.prefix);
        let vocab = self.model.vocab();
        let mut prompt = prefix.clone();
        prompt.extend(vocab.tokenize(self.template.messages(&req.messages).as_bytes(), false, true));
        let n_ctx = self.ctx.n_ctx() as usize;
        if prompt.len() + 64 > n_ctx {
            bail!("prompt ({} tokens) exceeds context size ({n_ctx})", prompt.len());
        }

        let cached = self.load_prompt(&req.prefix.cache_key, &prefix, &prompt)?;
        self.ctx.synchronize(); // decode is async on Metal; include the real compute time
        let mut stats = GenStats {
            cached_tokens: cached,
            prompt_tokens: prompt.len(),
            prompt_ms: ms_since(started),
            ..Default::default()
        };

        let mut sampler = if req.temperature <= 0.0 {
            LlamaSampler::greedy()
        } else {
            LlamaSampler::chain_simple([
                LlamaSampler::top_k(20),
                LlamaSampler::top_p(0.8, 1),
                LlamaSampler::temp(req.temperature),
                LlamaSampler::dist(rand_seed()),
            ])
        };

        let gen_start = Instant::now();
        let mut filter = StreamFilter::new(self.template.hidden_spans());
        let (mut raw, mut pending) = (String::new(), Vec::<u8>::new());
        let budget = (self.max_tokens as usize).min(n_ctx - self.kv.len());
        for _ in 0..budget {
            let token = sampler.sample(&self.ctx, -1);
            if vocab.is_eog(token) {
                break;
            }
            pending.extend(vocab.token_to_piece(token, true, None));
            let piece = take_utf8(&mut pending);
            raw.push_str(&piece);
            let visible = filter.push(&piece);
            if !visible.is_empty() && out.send(LlmEvent::Text(visible)).is_err() {
                return Ok(()); // receiver dropped: cancelled
            }
            self.decode(&[token])?;
            stats.gen_tokens += 1;
        }
        let (tail, text) = filter.finish();
        if !tail.is_empty() {
            let _ = out.send(LlmEvent::Text(tail));
        }
        stats.gen_ms = ms_since(gen_start);
        let (tool_calls, malformed) = self.template.parse_tool_calls(&raw);
        let _ =
            out.send(LlmEvent::Done(Completion { raw, text: text.trim().to_string(), tool_calls, malformed, stats }));
        Ok(())
    }

    /// Make the KV cache hold `prompt`, reusing whatever leading tokens are already
    /// resident. Returns the number of tokens that did not need recomputing.
    fn load_prompt(&mut self, key: &str, prefix: &[LlamaToken], prompt: &[LlamaToken]) -> Result<usize> {
        let mut keep = common_len(&self.kv, prompt);
        let mut reused = keep;
        if keep < prefix.len() {
            reused = if self.cache.get(key, prefix).is_some() { prefix.len() } else { 0 };
            self.ensure_prefix(key, prefix)?;
            keep = prefix.len();
        }
        keep = keep.min(prompt.len() - 1); // always evaluate at least one token to get logits
        self.ctx.clear_kv_cache_seq(Some(0), Some(keep as u32), None)?;
        self.kv.truncate(keep);
        self.decode(&prompt[keep..])?;
        Ok(reused.min(keep))
    }

    /// Put exactly `prefix` in the KV cache: restore its snapshot, or compute and snapshot it.
    fn ensure_prefix(&mut self, key: &str, prefix: &[LlamaToken]) -> Result<()> {
        if self.kv.starts_with(prefix) {
            return Ok(());
        }
        self.ctx.clear_kv_cache();
        self.kv.clear();
        if let Some(state) = self.cache.get(key, prefix) {
            self.ctx.state_seq_set(state, 0)?;
            self.kv = prefix.to_vec();
        } else {
            self.decode(prefix)?;
            let state = self.ctx.state_seq_get(0, LlamaStateSeqFlags::empty())?;
            self.cache.put(key, prefix.to_vec(), state);
        }
        Ok(())
    }

    /// Evaluate tokens at the end of the KV cache, in batch-sized chunks; logits for the last one.
    fn decode(&mut self, tokens: &[LlamaToken]) -> Result<()> {
        let n_batch = self.ctx.n_batch() as usize;
        let mut batch = LlamaBatch::new(n_batch, 1);
        for (c, chunk) in tokens.chunks(n_batch).enumerate() {
            batch.clear();
            let last_chunk = (c + 1) * n_batch >= tokens.len();
            for (i, &t) in chunk.iter().enumerate() {
                let pos = (self.kv.len() + i) as i32;
                batch.add(t, pos, &[0], last_chunk && i == chunk.len() - 1)?;
            }
            self.ctx.decode(&mut batch)?;
            self.kv.extend_from_slice(chunk);
        }
        Ok(())
    }
}

fn common_len(a: &[LlamaToken], b: &[LlamaToken]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

/// Split off the longest valid UTF-8 prefix (tokens can end mid-character).
fn take_utf8(bytes: &mut Vec<u8>) -> String {
    let valid = match std::str::from_utf8(bytes) {
        Ok(s) => s.len(),
        Err(e) if e.error_len().is_none() => e.valid_up_to(), // incomplete trailing char: wait for more
        Err(_) => bytes.len(),                                // genuinely invalid: emit lossily
    };
    let rest = bytes.split_off(valid);
    let s = String::from_utf8_lossy(bytes).into_owned();
    *bytes = rest;
    s
}

fn rand_seed() -> u32 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(42)
}
