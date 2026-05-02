# CLI reference

`mnem` is the single entry point. Subcommands wrap repo operations.

## Common subcommands

```bash
mnem init [path]                     # create .mnem/ in path (default: cwd)
mnem ingest <file|-> [...]           # add nodes from file or stdin
mnem retrieve <text> [...]           # query (vector + sparse + graph)
mnem mcp                             # start the MCP JSON-RPC server over stdio
mnem mcp --repo ~/notes              # point the MCP server at a specific graph
mnem http serve                      # start the HTTP JSON API (loopback by default)
mnem integrate                       # wire as MCP server in your agent host
mnem doctor                          # probe embedder + store + config
```

## Inspection

```bash
mnem stats                # commits, nodes, embeddings, store size
mnem log [-n N]           # commit history
mnem cat-file <cid>       # dump a node by CID
mnem diff <cid> <cid>     # diff two commits
mnem export               # export as CAR archive
```

## Advanced retrieve flags

```bash
--limit N                 # number of items to return (default 10); short: -n
--vector-cap N            # candidate pool from vector lane (default 256)
--graph-expand N          # multi-hop expansion budget
--graph-mode <decay|ppr>  # graph scoring: decay (default) or PPR
--rerank <provider:model> # post-rerank with a model
--summarize               # add community summarization layer
--community-filter        # Leiden community filter; drop low-coverage communities
```

## Ingest flags

```bash
--chunker <auto|paragraph|recursive|session>  # chunking strategy (default: auto)
--extractor keybert                            # enable KeyBERT keyphrase extraction
--max-tokens N                                # token budget per chunk (default: 512)
--recursive                                   # ingest a directory recursively
```

For complete option lists run `mnem <subcommand> --help`. Long-form
documentation for each subcommand lives in [guides](./guides/).
