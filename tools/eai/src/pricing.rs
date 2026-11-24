//! External updatable model pricing registry (`~/.eai/pricing.json` + `.eai/pricing.json` overrides)
//! and `eai pricing [--init] [--json]` command.

use crate::telemetry::user_home;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPriceEntry {
    pub pattern: String,
    pub agent: String, // "claude", "codex", "gemini", "antigravity", or "*"
    pub base_input_usd_per_m: f64,
    pub cache_read_mult: f64,
    pub cache_write_5m_mult: f64,
    pub cache_write_1h_mult: f64,
    pub output_mult: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingConfig {
    pub models: Vec<ModelPriceEntry>,
}

pub fn default_pricing() -> Vec<ModelPriceEntry> {
    vec![
        ModelPriceEntry {
            pattern: "opus".into(),
            agent: "claude".into(),
            base_input_usd_per_m: 15.0,
            cache_read_mult: 0.10,
            cache_write_5m_mult: 1.25,
            cache_write_1h_mult: 2.0,
            output_mult: 5.0,
        },
        ModelPriceEntry {
            pattern: "haiku".into(),
            agent: "claude".into(),
            base_input_usd_per_m: 0.80,
            cache_read_mult: 0.10,
            cache_write_5m_mult: 1.25,
            cache_write_1h_mult: 2.0,
            output_mult: 5.0,
        },
        ModelPriceEntry {
            pattern: "sonnet".into(),
            agent: "claude".into(),
            base_input_usd_per_m: 3.0,
            cache_read_mult: 0.10,
            cache_write_5m_mult: 1.25,
            cache_write_1h_mult: 2.0,
            output_mult: 5.0,
        },
        ModelPriceEntry {
            pattern: "flash-lite".into(),
            agent: "*".into(),
            base_input_usd_per_m: 0.10,
            cache_read_mult: 0.25,
            cache_write_5m_mult: 1.0,
            cache_write_1h_mult: 1.0,
            output_mult: 4.0,
        },
        ModelPriceEntry {
            pattern: "flash".into(),
            agent: "*".into(),
            base_input_usd_per_m: 0.30,
            cache_read_mult: 0.25,
            cache_write_5m_mult: 1.0,
            cache_write_1h_mult: 1.0,
            output_mult: 4.0,
        },
        ModelPriceEntry {
            pattern: "gemini".into(),
            agent: "*".into(),
            base_input_usd_per_m: 1.25,
            cache_read_mult: 0.25,
            cache_write_5m_mult: 1.0,
            cache_write_1h_mult: 1.0,
            output_mult: 4.0,
        },
        ModelPriceEntry {
            pattern: "mini".into(),
            agent: "codex".into(),
            base_input_usd_per_m: 0.25,
            cache_read_mult: 0.50,
            cache_write_5m_mult: 1.0,
            cache_write_1h_mult: 1.0,
            output_mult: 4.0,
        },
        ModelPriceEntry {
            pattern: "codex".into(),
            agent: "codex".into(),
            base_input_usd_per_m: 1.50,
            cache_read_mult: 0.50,
            cache_write_5m_mult: 1.0,
            cache_write_1h_mult: 1.0,
            output_mult: 4.0,
        },
        ModelPriceEntry {
            pattern: "gpt".into(),
            agent: "codex".into(),
            base_input_usd_per_m: 1.50,
            cache_read_mult: 0.50,
            cache_write_5m_mult: 1.0,
            cache_write_1h_mult: 1.0,
            output_mult: 4.0,
        },
    ]
}

fn load_pricing_entries() -> Vec<ModelPriceEntry> {
    let mut entries = Vec::new();
    // 1. Project override `.eai/pricing.json` takes highest priority
    for p in [
        Some(PathBuf::from(".eai").join("pricing.json")),
        user_home().map(|h| h.join(".eai").join("pricing.json")),
    ]
    .into_iter()
    .flatten()
    {
        if p.is_file() {
            if let Ok(txt) = fs::read_to_string(&p) {
                if let Ok(cfg) = serde_json::from_str::<PricingConfig>(&txt) {
                    entries.extend(cfg.models);
                }
            }
        }
    }
    // 2. Built-in defaults as fallback
    entries.extend(default_pricing());
    entries
}

static PRICING_CACHE: OnceLock<Vec<ModelPriceEntry>> = OnceLock::new();

pub fn active_pricing() -> &'static [ModelPriceEntry] {
    PRICING_CACHE.get_or_init(load_pricing_entries).as_slice()
}

/// Lookup base USD per 1M uncached input tokens using external overrides first, then built-in defaults.
pub fn resolve_base_usd_per_m(model: &str, agent: &str) -> f64 {
    let m = model.to_ascii_lowercase();
    for entry in active_pricing() {
        let agent_ok = entry.agent == "*" || entry.agent.eq_ignore_ascii_case(agent);
        if agent_ok && m.contains(&entry.pattern.to_ascii_lowercase()) {
            return entry.base_input_usd_per_m;
        }
    }
    match agent {
        "claude" => 3.0,
        "gemini" | "antigravity" => 1.25,
        "codex" => 1.50,
        _ => 2.0,
    }
}

pub fn cmd_pricing(args: &[String]) -> i32 {
    let mut init = false;
    let mut json_out = false;
    for a in args {
        match a.as_str() {
            "--init" => init = true,
            "--json" => json_out = true,
            "-h" | "--help" => {
                println!(
                    "Usage: eai pricing [--init] [--json]\n\
                     Inspect active model pricing table, or run `--init` to create an editable `~/.eai/pricing.json` override file."
                );
                return 0;
            }
            _ => {}
        }
    }

    if init {
        let Some(h) = user_home() else {
            eprintln!("Could not determine home directory.");
            return 1;
        };
        let target = h.join(".eai").join("pricing.json");
        if let Some(p) = target.parent() {
            let _ = fs::create_dir_all(p);
        }
        let cfg = PricingConfig {
            models: default_pricing(),
        };
        if let Err(e) = fs::write(
            &target,
            format!("{}\n", serde_json::to_string_pretty(&cfg).unwrap_or_default()),
        ) {
            eprintln!("Error writing {}: {}", target.display(), e);
            return 1;
        }
        println!(
            "Created editable pricing override file at `{}`.\nEdit rates or add custom models anytime — `eai analyze`, `eai impact`, and `eai dashboard` pick it up automatically.",
            target.display()
        );
        return 0;
    }

    let entries = active_pricing();
    if json_out {
        println!("{}", serde_json::to_string_pretty(&entries).unwrap_or_default());
        return 0;
    }

    println!("# efficient-ai Active Model Pricing Registry\n");
    println!("| Pattern | Agent | Base Input ($/1M) | Cache Read | Cache Write 5m/1h | Output |");
    println!("|---|---|---:|---:|---:|---:|");
    for e in entries {
        println!(
            "| `{}` | {} | ${:.2} | {:.2}× | {:.2}× / {:.2}× | {:.1}× |",
            e.pattern,
            e.agent,
            e.base_input_usd_per_m,
            e.cache_read_mult,
            e.cache_write_5m_mult,
            e.cache_write_1h_mult,
            e.output_mult
        );
    }
    println!("\n*Run `eai pricing --init` to create `~/.eai/pricing.json` for custom/enterprise rates.*");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_pricing_resolution() {
        assert!((resolve_base_usd_per_m("claude-opus-4-6", "claude") - 15.0).abs() < 0.01);
        assert!((resolve_base_usd_per_m("claude-sonnet-5", "claude") - 3.0).abs() < 0.01);
        assert!((resolve_base_usd_per_m("gemini-2.5-flash", "gemini") - 0.30).abs() < 0.01);
        assert!((resolve_base_usd_per_m("gpt-5-codex", "codex") - 1.50).abs() < 0.01);
    }
}
