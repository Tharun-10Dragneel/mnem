use super::*;

use crate::{config, global, repo};

/// `mnem global` subcommand - operations across all registered repos and the
/// global anchor graph at `~/.mnemglobal/.mnem/`.
#[derive(clap::Subcommand, Debug)]
pub(crate) enum GlobalCmd {
    /// Search ALL registered repos + the global graph simultaneously.
    /// Results are merged, deduplicated by node UUID, and ranked by score.
    /// Each result is labelled with its source repo path.
    #[command(after_long_help = "\
Examples:
  mnem global retrieve \"Alice in Berlin\"
  mnem global retrieve \"climbing\" -n 5
  mnem global retrieve \"project deadline\" --no-vector
")]
    Retrieve(GlobalRetrieveArgs),

    /// Add a node or edge directly to the global graph (~/.mnemglobal/.mnem/).
    /// The printed node UUID can be used as `--prop _global_anchor=<uuid>` when
    /// adding the same entity to a local repo, linking the two graphs.
    ///
    /// Examples:
    ///   mnem global add node -s \"Alice works at Anthropic\" --label Entity:Person --prop name=Alice
    ///   # -> prints: node <uuid> committed
    ///   mnem -R ~/notes add node --label Entity:Person --prop name=Alice --prop _global_anchor=<uuid>
    #[command(subcommand)]
    Add(super::add::AddCmd),
}

#[derive(clap::Args, Debug)]
pub(crate) struct GlobalRetrieveArgs {
    /// Query text. Embedded with the configured provider and used for
    /// semantic search across all repos.
    #[arg(value_name = "QUERY")]
    pub query: Option<String>,
    /// Max results per repo before merging (total may be up to N * repos).
    /// Final output is truncated to N after cross-repo merge. Default: 10.
    #[arg(long, short = 'n')]
    pub limit: Option<usize>,
    /// Skip semantic (vector) search. Useful when no embedder is configured.
    #[arg(long)]
    pub no_vector: bool,
}

pub(crate) fn run(_override: Option<&Path>, cmd: GlobalCmd) -> Result<()> {
    let global_dir = global::default_dir();
    match cmd {
        GlobalCmd::Retrieve(args) => cmd_retrieve(&global_dir, args),
        GlobalCmd::Add(add_cmd) => {
            if !global_dir.join(repo::MNEM_DIR).is_dir() {
                bail!(
                    "Global graph not initialised at {}.\n\
                     hint: run `mnem integrate` to create it.",
                    global_dir.display()
                );
            }
            super::add::run(Some(&global_dir), add_cmd)
        }
    }
}

struct Hit {
    score: f32,
    tokens: u32,
    node: mnem_core::objects::Node,
    source: std::path::PathBuf,
}

fn cmd_retrieve(global_dir: &Path, args: GlobalRetrieveArgs) -> Result<()> {
    let query = args.query.as_deref().unwrap_or("").trim().to_string();
    if query.is_empty() {
        bail!("A query text is required.\nUsage: mnem global retrieve \"your query\"");
    }
    let limit = args.limit.unwrap_or(10);

    // --- build target list: global graph first, then registered repos ---
    let mut targets: Vec<std::path::PathBuf> = Vec::new();
    if global_dir.join(repo::MNEM_DIR).is_dir() {
        targets.push(global_dir.to_path_buf());
    }
    if global_dir.exists() {
        if let Ok(reg) = global::RepoRegistry::load(global_dir) {
            for entry in reg.repos {
                let candidate = entry.path.clone();
                if candidate.join(repo::MNEM_DIR).is_dir() && !targets.contains(&candidate) {
                    targets.push(candidate);
                }
            }
        }
    }
    if targets.is_empty() {
        bail!(
            "No repos to search.\n\
             hint: run `mnem integrate` then `mnem init <path>` to register repos."
        );
    }

    // --- embed query once, using the first repo that has an embedder configured ---
    let opt_vec: Option<(String, Vec<f32>)> = if args.no_vector {
        None
    } else {
        embed_query_once(&targets, &query)
    };

    // --- search each repo, collect hits ---
    let mut all_hits: Vec<Hit> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for target in &targets {
        let r = match repo::open_repo(Some(target.as_path())) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("(skipping {}: {e})", target.display());
                continue;
            }
        };
        let mut ret = r.retrieve().limit(limit).query_text(query.clone());
        if let Some((model, vec)) = &opt_vec {
            ret = ret.vector(model.clone(), vec.clone());
        }
        match ret.execute() {
            Ok(result) => {
                for item in result.items {
                    let id = item.node.id.to_uuid_string();
                    if seen.insert(id) {
                        all_hits.push(Hit {
                            score: item.score,
                            tokens: item.tokens,
                            node: item.node,
                            source: target.clone(),
                        });
                    }
                }
            }
            Err(e) => eprintln!("(error searching {}: {e})", target.display()),
        }
    }

    if all_hits.is_empty() {
        println!("No results found across {} repo(s).", targets.len());
        return Ok(());
    }

    // sort by score desc, then truncate to limit
    all_hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    all_hits.truncate(limit);

    println!(
        "Found {} result(s) across {} repo(s):\n",
        all_hits.len(),
        targets.len()
    );
    for (i, hit) in all_hits.iter().enumerate() {
        let src = if hit.source == *global_dir {
            "[global]".to_string()
        } else {
            format!("[{}]", hit.source.display())
        };
        println!(
            "---\n[{i}] score={:.4} tokens={} {} id={} {}",
            hit.score,
            hit.tokens,
            src,
            hit.node.id.to_uuid_string(),
            hit.node.ntype,
        );
        if let Some(s) = &hit.node.summary {
            println!("  {}", s.lines().next().unwrap_or(""));
        }
        if !hit.node.props.is_empty() {
            let preview: Vec<String> = hit
                .node
                .props
                .iter()
                .take(3)
                .map(|(k, v)| format!("{k}={}", ipld_preview(v)))
                .collect();
            println!("  props: {}", preview.join(", "));
        }
    }
    Ok(())
}

/// Try to embed `query` using the first repo in `targets` that has a working
/// embedder configured. Returns `None` silently if none are configured or all
/// fail (the caller proceeds with property-only retrieval).
fn embed_query_once(
    targets: &[std::path::PathBuf],
    query: &str,
) -> Option<(String, Vec<f32>)> {
    for target in targets {
        let data_dir = target.join(repo::MNEM_DIR);
        let cfg = config::load(&data_dir).ok()?;
        let Some(pc) = config::resolve_embedder(&cfg) else {
            continue;
        };
        match mnem_embed_providers::open(&pc) {
            Ok(embedder) => match embedder.embed(query) {
                Ok(v) => return Some((embedder.model().to_string(), v)),
                Err(e) => {
                    eprintln!("{}", format_embed_failure(&e, &pc, "query embedding"));
                    continue;
                }
            },
            Err(e) => {
                eprintln!("{}", format_embed_failure(&e, &pc, "query embedding"));
                continue;
            }
        }
    }
    None
}
