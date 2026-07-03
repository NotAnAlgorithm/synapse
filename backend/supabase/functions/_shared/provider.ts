// Provider abstraction — LLM Generator + Embedder behind thin interfaces.
//
// Rationale (M2 design §4.1): model choice matters far less than *grounding*, so
// provider / model-id / prompt templates are CONFIG, not code. The owner already
// has an LLM provider; slot it in behind these interfaces and select it via env
// (see .env.example). No keys ship in this repo; the stubs below throw until a
// real provider is wired, so nothing silently runs ungrounded.
//
// NOTE: the flaw checker is deliberately NOT here — it is deterministic rule-based
// code (see flaw_check.ts), never an LLM judge (M2 design §4.3; PRD C1: rule-based
// catches ~91% of flaws vs ~79% for an LLM judge).

/** One chat message in the provider-neutral shape. */
export interface Message {
  role: "system" | "user" | "assistant" | "tool";
  content: string;
}

/** Provider-neutral tool/function description (optional; unused by C1 draft). */
export interface ToolSpec {
  name: string;
  description: string;
  // JSON-schema-ish parameter object; kept opaque at this layer.
  parameters: Record<string, unknown>;
}

/** A tool call the model requested (returned alongside/instead of text). */
export interface ToolCall {
  name: string;
  arguments: Record<string, unknown>;
}

export interface CompletionResult {
  text: string;
  toolCalls: ToolCall[];
}

/**
 * Generator: drafts text from grounded messages. The grounding contract lives in
 * the CALLER (generate/index.ts builds a system prompt that constrains the model
 * to transform only the retrieved chunk text). This interface is just transport.
 */
export interface Generator {
  complete(messages: Message[], tools?: ToolSpec[]): Promise<CompletionResult>;
}

/** Embedder: turns text into vectors for corpus indexing + query-time ranking. */
export interface Embedder {
  /** Embedding dimension — MUST match the vector column in the migration. */
  readonly dimension: number;
  embed(texts: string[]): Promise<number[][]>;
}

// ---------------------------------------------------------------------------
// Config — read from the environment; NO secrets in the repo. See .env.example.
// ---------------------------------------------------------------------------

export interface ProviderConfig {
  provider: string; // logical provider name, e.g. "stub" | "<owner-provider>"
  generatorModel: string;
  embedderModel: string;
  embeddingDimension: number;
  apiKey: string | undefined; // never hard-coded; injected as a function secret
}

function env(name: string): string | undefined {
  // Deno runtime in Supabase Edge Functions.
  return (globalThis as { Deno?: { env: { get(k: string): string | undefined } } })
    .Deno?.env.get(name);
}

export function loadProviderConfig(): ProviderConfig {
  return {
    provider: env("SYNAPSE_PROVIDER") ?? "stub",
    generatorModel: env("SYNAPSE_GENERATOR_MODEL") ?? "stub-generator",
    embedderModel: env("SYNAPSE_EMBEDDER_MODEL") ?? "stub-embedder",
    // Keep in sync with supabase/migrations/0001_init_corpus.sql (vector(1536)).
    embeddingDimension: Number(env("SYNAPSE_EMBEDDING_DIM") ?? "1536"),
    apiKey: env("SYNAPSE_PROVIDER_API_KEY"),
  };
}

// ---------------------------------------------------------------------------
// Config-driven stubs. These are structural placeholders (provider TBD): they
// throw a clear error instead of returning fake content, so the pipeline cannot
// accidentally "work" ungrounded before a real provider is configured.
// ---------------------------------------------------------------------------

class StubGenerator implements Generator {
  complete(_messages: Message[], _tools?: ToolSpec[]): Promise<CompletionResult> {
    return Promise.reject(
      new Error(
        "No LLM provider configured. Set SYNAPSE_PROVIDER + credentials and " +
          "implement the provider adapter in _shared/provider.ts.",
      ),
    );
  }
}

class StubEmbedder implements Embedder {
  readonly dimension: number;
  constructor(dimension: number) {
    this.dimension = dimension;
  }
  embed(_texts: string[]): Promise<number[][]> {
    return Promise.reject(
      new Error(
        "No embedding provider configured. Set SYNAPSE_EMBEDDER_MODEL + " +
          "credentials and implement the embedder adapter in _shared/provider.ts.",
      ),
    );
  }
}

/**
 * Factory: return the configured Generator. Add a `case "<owner-provider>"` here
 * that constructs the real adapter from `cfg` when the provider is chosen.
 */
export function makeGenerator(cfg: ProviderConfig = loadProviderConfig()): Generator {
  switch (cfg.provider) {
    // case "<owner-provider>":
    //   return new OwnerProviderGenerator(cfg);
    case "stub":
    default:
      return new StubGenerator();
  }
}

/** Factory: return the configured Embedder (dimension must match the DB). */
export function makeEmbedder(cfg: ProviderConfig = loadProviderConfig()): Embedder {
  switch (cfg.provider) {
    // case "<owner-provider>":
    //   return new OwnerProviderEmbedder(cfg);
    case "stub":
    default:
      return new StubEmbedder(cfg.embeddingDimension);
  }
}
