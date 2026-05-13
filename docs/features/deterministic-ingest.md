# Deterministic Ingest

mnem ingests documents without an LLM. The parse, chunk, and entity extraction pipeline is statistical - the same bytes always produce the same nodes, with the same CIDs, on any machine.

```bash
mnem ingest architecture.md        # parses into Doc + Chunk + Entity nodes
mnem ingest --recursive docs/      # ingest a directory
```

## Why no LLM at ingest

LLM-based extraction is non-deterministic: two runs on the same document may produce different entities, different summaries, different chunks. This breaks reproducibility and makes auditing hard.

mnem's ingest pipeline is:

1. **Parse** - extract text from the file format (Markdown, PDF, plain text, etc.)
2. **Chunk** - split into semantically coherent segments using configurable chunkers
3. **Extract** - identify entities using statistical NER (no network call)
4. **Embed** - compute dense vectors with the configured embedder (in-process by default)
5. **Commit** - write the resulting nodes and edges as a single content-addressed commit

Every step is deterministic. Run the same ingest twice and you get the same CIDs.

## Optional keyphrase enrichment

For stronger sparse retrieval signal, KeyBERT extraction is available at ingest time:

```bash
mnem ingest --extractor keybert notes.md
```

KeyBERT uses a local model - no LLM call, no network required.

## Fuzz testing

The ingest parsers (Markdown, PDF, plain text) are fuzz-harnessed. Malformed or adversarial input is handled safely without panics.

## See also

- [Ingest pipeline](../src/guides/ingest.md)
- [mnem ingest CLI reference](../src/cli.md)
