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
  provider: string; // logical provider name, e.g. "stub" | "openai"
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

// ---------------------------------------------------------------------------
// OpenAI adapter. Calls the Chat Completions + Embeddings REST endpoints via
// fetch (available in the Deno Edge runtime) — no SDK dependency. Model ids come
// from config (SYNAPSE_GENERATOR_MODEL / SYNAPSE_EMBEDDER_MODEL) so switching
// models is env-only. Set SYNAPSE_PROVIDER_BASE_URL to target an
// OpenAI-compatible endpoint (Azure, a gateway/proxy); it defaults to the public
// API. Grounding is still enforced by the CALLER (generate/index.ts) — this is
// only transport.
// ---------------------------------------------------------------------------

const DEFAULT_OPENAI_BASE_URL = "https://api.openai.com/v1";

function openAiBaseUrl(): string {
  return (env("SYNAPSE_PROVIDER_BASE_URL") ?? DEFAULT_OPENAI_BASE_URL).replace(/\/+$/, "");
}

function requireApiKey(cfg: ProviderConfig): void {
  if (!cfg.apiKey) {
    throw new Error(
      "SYNAPSE_PROVIDER_API_KEY is required for the OpenAI provider (set it in " +
        ".env for local scripts, or as a Supabase function secret in deployment).",
    );
  }
}

interface OpenAiChatResponse {
  choices?: Array<{
    message?: {
      content?: string | null;
      tool_calls?: Array<{ function?: { name?: string; arguments?: string } }>;
    };
  }>;
}

interface OpenAiEmbeddingResponse {
  data?: Array<{ index: number; embedding: number[] }>;
}

function parseToolArguments(raw: string | undefined): Record<string, unknown> {
  if (!raw) return {};
  try {
    return JSON.parse(raw) as Record<string, unknown>;
  } catch {
    return {};
  }
}

class OpenAiGenerator implements Generator {
  constructor(private readonly cfg: ProviderConfig) {
    requireApiKey(cfg);
  }

  async complete(messages: Message[], tools?: ToolSpec[]): Promise<CompletionResult> {
    const body: Record<string, unknown> = {
      model: this.cfg.generatorModel,
      messages: messages.map((m) => ({ role: m.role, content: m.content })),
    };
    if (tools && tools.length > 0) {
      body.tools = tools.map((t) => ({
        type: "function",
        function: { name: t.name, description: t.description, parameters: t.parameters },
      }));
    }

    const res = await fetch(`${openAiBaseUrl()}/chat/completions`, {
      method: "POST",
      headers: {
        "Authorization": `Bearer ${this.cfg.apiKey}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      throw new Error(`OpenAI chat/completions failed (${res.status}): ${await res.text()}`);
    }

    const data = (await res.json()) as OpenAiChatResponse;
    const message = data.choices?.[0]?.message ?? {};
    const toolCalls: ToolCall[] = (message.tool_calls ?? []).map((tc) => ({
      name: tc.function?.name ?? "",
      arguments: parseToolArguments(tc.function?.arguments),
    }));
    return { text: message.content ?? "", toolCalls };
  }
}

class OpenAiEmbedder implements Embedder {
  readonly dimension: number;

  constructor(private readonly cfg: ProviderConfig) {
    requireApiKey(cfg);
    this.dimension = cfg.embeddingDimension;
  }

  async embed(texts: string[]): Promise<number[][]> {
    if (texts.length === 0) return [];

    const body: Record<string, unknown> = {
      model: this.cfg.embedderModel,
      input: texts,
    };
    // text-embedding-3-* honour `dimensions`; pin it so returned vectors match
    // the pgvector column width (SYNAPSE_EMBEDDING_DIM / the migration).
    if (this.dimension > 0) body.dimensions = this.dimension;

    const res = await fetch(`${openAiBaseUrl()}/embeddings`, {
      method: "POST",
      headers: {
        "Authorization": `Bearer ${this.cfg.apiKey}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      throw new Error(`OpenAI embeddings failed (${res.status}): ${await res.text()}`);
    }

    const data = (await res.json()) as OpenAiEmbeddingResponse;
    // Sort by index so output order matches input order.
    const rows = (data.data ?? []).slice().sort((a, b) => a.index - b.index);
    const vectors = rows.map((row) => row.embedding);
    if (vectors.length !== texts.length) {
      throw new Error(
        `OpenAI embeddings returned ${vectors.length} vectors for ${texts.length} inputs`,
      );
    }
    return vectors;
  }
}

/**
 * Factory: return the configured Generator. `openai` is implemented (Chat
 * Completions); add another `case` to support a different provider.
 */
export function makeGenerator(cfg: ProviderConfig = loadProviderConfig()): Generator {
  switch (cfg.provider) {
    case "openai":
      return new OpenAiGenerator(cfg);
    case "stub":
    default:
      return new StubGenerator();
  }
}

/** Factory: return the configured Embedder (dimension must match the DB). */
export function makeEmbedder(cfg: ProviderConfig = loadProviderConfig()): Embedder {
  switch (cfg.provider) {
    case "openai":
      return new OpenAiEmbedder(cfg);
    case "stub":
    default:
      return new StubEmbedder(cfg.embeddingDimension);
  }
}
