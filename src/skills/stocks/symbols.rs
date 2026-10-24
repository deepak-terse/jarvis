//! Spoken company or index name → exchange listing. Config aliases win, then names
//! learned from earlier searches (saved to disk), then a fresh symbol search.

use super::twelve_data::Match;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Listing {
    pub symbol: String,
    pub exchange: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    symbols: BTreeMap<String, Listing>,
}

pub struct Symbols {
    aliases: BTreeMap<String, Listing>,
    learned: Mutex<Saved>,
    path: PathBuf,
}

impl Symbols {
    /// `aliases` maps a spoken name to `SYMBOL` or `SYMBOL:EXCHANGE` (default: `default_exchange`).
    pub fn load(path: PathBuf, aliases: &BTreeMap<String, String>, default_exchange: &str) -> Result<Self> {
        let aliases = aliases
            .iter()
            .map(|(name, target)| {
                let (symbol, exchange) = target.rsplit_once(':').unwrap_or((target, default_exchange));
                (normalize(name), Listing { symbol: symbol.trim().into(), exchange: exchange.trim().into() })
            })
            .collect();
        let learned = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!("ignoring unreadable {}: {e}", path.display());
                Saved::default()
            }),
            Err(_) => Saved::default(),
        };
        Ok(Self { aliases, learned: Mutex::new(learned), path })
    }

    pub fn lookup(&self, spoken: &str) -> Option<Listing> {
        let key = normalize(spoken);
        self.aliases.get(&key).cloned().or_else(|| self.learned.lock().unwrap().symbols.get(&key).cloned())
    }

    /// Remember a resolution for next time. Saving is best effort.
    pub fn learn(&self, spoken: &str, listing: Listing) {
        let mut learned = self.learned.lock().unwrap();
        learned.symbols.insert(normalize(spoken), listing);
        let result = (|| -> Result<()> {
            if let Some(dir) = self.path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&self.path, toml::to_string(&*learned)?)?;
            Ok(())
        })();
        if let Err(e) = result {
            tracing::warn!("could not save {}: {e}", self.path.display());
        }
    }
}

/// "Reliance Industries shares" → "reliance industries".
pub fn normalize(spoken: &str) -> String {
    const FILLER: &[&str] = &["share", "shares", "stock", "stocks", "price", "ltd", "limited", "the", "index", "today"];
    spoken
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '&')
        .filter(|w| !w.is_empty() && !FILLER.contains(w))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Pick the best search result: preferred exchanges first (in order), then names that
/// contain every spoken word, then stocks and indices over other instruments, then
/// the provider's own relevance order.
pub fn best_match<'a>(
    spoken: &str,
    matches: &'a [Match],
    exchanges: &[String],
    any_exchange: bool,
) -> Option<&'a Match> {
    let words: Vec<String> = normalize(spoken).split(' ').map(str::to_string).collect();
    matches
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            let exchange_rank = exchanges.iter().position(|e| e.eq_ignore_ascii_case(&m.exchange));
            if exchange_rank.is_none() && !any_exchange {
                return None;
            }
            let name = m.instrument_name.to_lowercase();
            let named =
                m.symbol.eq_ignore_ascii_case(&words.join("")) || words.iter().all(|w| name.contains(w.as_str()));
            let kind = match m.instrument_type.as_str() {
                "Common Stock" | "Index" => 0,
                "ETF" | "Depositary Receipt" => 1,
                _ => 2,
            };
            Some(((exchange_rank.unwrap_or(exchanges.len()), !named, kind, i), m))
        })
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, m)| m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(symbol: &str, name: &str, exchange: &str, kind: &str) -> Match {
        Match {
            symbol: symbol.into(),
            instrument_name: name.into(),
            exchange: exchange.into(),
            instrument_type: kind.into(),
        }
    }

    fn exchanges() -> Vec<String> {
        vec!["NSE".into(), "BSE".into()]
    }

    #[test]
    fn prefers_indian_exchanges_and_name_matches() {
        let results = [
            m("RELI", "Reliance Global Group", "NASDAQ", "Common Stock"),
            m("RIGD", "Reliance Industries GDR", "LSE", "Depositary Receipt"),
            m("RELIANCE", "Reliance Industries Limited", "BSE", "Common Stock"),
            m("RELIANCE", "Reliance Industries Limited", "NSE", "Common Stock"),
            m("RPOWER", "Reliance Power Limited", "NSE", "Common Stock"),
        ];
        let best = best_match("Reliance Industries", &results, &exchanges(), false).unwrap();
        assert_eq!((best.symbol.as_str(), best.exchange.as_str()), ("RELIANCE", "NSE"));
        let foreign = [m("AAPL", "Apple Inc", "NASDAQ", "Common Stock")];
        assert!(best_match("apple", &foreign, &exchanges(), false).is_none());
        assert_eq!(best_match("apple", &foreign, &exchanges(), true).unwrap().symbol, "AAPL");
    }

    #[test]
    fn aliases_then_learned_names_persist() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("skills/stock_symbols.toml");
        let aliases = BTreeMap::from([
            ("Nifty".to_string(), "NIFTY 50".to_string()),
            ("Sensex".to_string(), "SENSEX:BSE".to_string()),
        ]);
        let s = Symbols::load(path.clone(), &aliases, "NSE").unwrap();
        assert_eq!(s.lookup("nifty"), Some(Listing { symbol: "NIFTY 50".into(), exchange: "NSE".into() }));
        assert_eq!(s.lookup("the Sensex index").unwrap().exchange, "BSE");
        assert_eq!(s.lookup("Reliance shares"), None);

        s.learn("Reliance", Listing { symbol: "RELIANCE".into(), exchange: "NSE".into() });
        let reloaded = Symbols::load(path, &BTreeMap::new(), "NSE").unwrap();
        assert_eq!(reloaded.lookup("reliance share price").unwrap().symbol, "RELIANCE");
    }
}
