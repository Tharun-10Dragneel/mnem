//! Handler for `mnem_global_retrieve` - cross-repo semantic search.
//!
//! Opens every repo registered in `~/.mnemglobal/repos.toml` plus the
//! global anchor graph itself, runs the standard retriever pipeline on
//! each, deduplicates results by node UUID, and returns them ranked by
//! score with a `[source]` label so the caller knows which graph each
//! hit came from.

use crate::server::Server;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

// ---------- minimal registry types ----------

#[derive(Debug, Deserialize, Serialize)]
struct RepoEntry {
    path: PathBuf,
    #[serde(default)]
    default: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RepoRegistry {
    #[serde(default)]
    repos: Vec<RepoEntry>,
}

impl RepoRegistry {
    fn load(global_dir: &std::path::Path) -> Self {
        let path = global_dir.join("repos.toml");
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => return Self::default(),
        };
        toml::from_str(&text).unwrap_or_default()
    }
}

// ---------- handler ----------

pub(in crate::tools) fn global_retrieve(_server: &mut Server, args: Value) -> Result<String> {
    let global_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".mnemglobal");

    // Build target list: global graph first, then registered repos.
    let mut targets: Vec<(PathBuf, String)> = Vec::new(); // (parent_dir, label)
    let global_data = global_dir.join(".mnem");
    if global_data.is_dir() {
        targets.push((global_dir.clone(), "[global]".to_string()));
    }
    let reg = RepoRegistry::load(&global_dir);
    for entry in &reg.repos {
        let candidate = entry.path.clone();
        if candidate.join(".mnem").is_dir() && !targets.iter().any(|(p, _)| p == &candidate) {
            let src_label = if let Some(lbl) = &entry.label {
                format!("[{}]", lbl)
            } else {
                format!("[{}]", candidate.display())
            };
            targets.push((candidate, src_label));
        }
    }

    if targets.is_empty() {
        return Ok(
            "mnem_global_retrieve: no repos registered. Run `mnem integrate` then `mnem init`.\n"
                .to_string(),
        );
    }

    // Parse shared args.
    let text_arg = args.get("text").and_then(Value::as_str).map(str::to_string);
    let vector_arg = args.get("vector").and_then(Value::as_object).cloned();
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|n| (n as usize).min(super::super::MAX_RETRIEVE_LIMIT))
        .unwrap_or(10);
    let budget = args
        .get("token_budget")
        .and_then(Value::as_u64)
        .map(|n| n.min(u64::from(u32::MAX)) as u32);

    // Parse pre-computed vector once (re-used across all repos).
    let opt_vec: Option<(String, Vec<f32>)> = if let Some(vec_obj) = vector_arg {
        let model = vec_obj
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let vals = vec_obj
            .get("values")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_f64().map(|f| f as f32))
                    .collect::<Vec<f32>>()
            })
            .unwrap_or_default();
        if !model.is_empty() && !vals.is_empty() {
            Some((model, vals))
        } else {
            None
        }
    } else {
        // Try to auto-embed using the global graph's config, then fall
        // through to other repos.
        #[cfg(feature = "summarize")]
        {
            if let Some(ref text) = text_arg {
                let mut found = None;
                for (parent, _) in &targets {
                    let data_dir = parent.join(".mnem");
                    if let Some(cfg) = crate::tools::embed::resolve_embed_cfg(&data_dir)
                        && let Ok(embedder) = mnem_embed_providers::open(&cfg)
                        && let Ok(vec) = embedder.embed(text)
                    {
                        found = Some((embedder.model().to_string(), vec));
                        break;
                    }
                }
                found
            } else {
                None
            }
        }
        #[cfg(not(feature = "summarize"))]
        None
    };

    // Search each target repo, collect hits.
    struct Hit {
        score: f32,
        tokens: u32,
        rendered: String,
        id: String,
        ntype: String,
        source: String,
    }

    let mut all_hits: Vec<Hit> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (parent, src_label) in &targets {
        let repo = match Server::open_repo_at(&parent.join(".mnem")) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("(mnem_global_retrieve: skipping {}: {e})", parent.display());
                continue;
            }
        };

        let mut r = repo.retrieve().limit(limit);
        if let Some(ref text) = text_arg {
            r = r.query_text(text.clone());
        }
        if let Some((ref model, ref vec)) = opt_vec {
            r = r.vector(model.clone(), vec.clone());
        }
        if let Some(b) = budget {
            r = r.token_budget(b);
        }

        let result = match r.execute() {
            Ok(res) => res,
            Err(e) => {
                let msg = format!("{e:#}");
                if msg.contains("no filters or rankers configured") {
                    continue; // empty repo / no config - skip silently
                }
                eprintln!("(mnem_global_retrieve: error in {}: {e})", parent.display());
                continue;
            }
        };

        for item in result.items {
            let id = item.node.id.to_uuid_string();
            if seen.insert(id.clone()) {
                all_hits.push(Hit {
                    score: item.score,
                    tokens: item.tokens,
                    rendered: item.rendered.clone(),
                    id,
                    ntype: item.node.ntype.clone(),
                    source: src_label.clone(),
                });
            }
        }
    }

    if all_hits.is_empty() {
        return Ok(format!(
            "mnem_global_retrieve: 0 item(s) across {} repo(s)\n",
            targets.len()
        ));
    }

    // Sort by score desc, truncate.
    all_hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    all_hits.truncate(limit);

    let mut out = String::new();
    out.push_str(&format!(
        "mnem_global_retrieve: {} item(s) across {} repo(s)\n",
        all_hits.len(),
        targets.len()
    ));
    for (i, hit) in all_hits.iter().enumerate() {
        out.push_str(&format!(
            "  [{i}] score={:.4} tokens={} {} id={} {}\n",
            hit.score, hit.tokens, hit.source, hit.id, hit.ntype,
        ));
        for line in hit.rendered.lines() {
            out.push_str(&format!("        {line}\n"));
        }
    }
    Ok(out)
}
