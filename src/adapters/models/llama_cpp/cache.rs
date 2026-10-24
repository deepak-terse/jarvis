//! KV snapshots of each persona's stable prompt prefix, so switching persona
//! restores the system prompt instead of recomputing it.

use llama_cpp_2::SeqState;
use llama_cpp_2::token::LlamaToken;
use std::collections::HashMap;

#[derive(Default)]
pub struct PrefixCache {
    entries: HashMap<String, (Vec<LlamaToken>, SeqState)>,
}

impl PrefixCache {
    /// The snapshot for `key`, if it was taken for exactly these tokens.
    pub fn get(&self, key: &str, tokens: &[LlamaToken]) -> Option<&SeqState> {
        self.entries.get(key).filter(|(t, _)| t == tokens).map(|(_, s)| s)
    }

    pub fn put(&mut self, key: &str, tokens: Vec<LlamaToken>, state: SeqState) {
        self.entries.insert(key.to_string(), (tokens, state));
    }
}
