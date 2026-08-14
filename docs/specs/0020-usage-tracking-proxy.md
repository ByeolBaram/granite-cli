# Plan: Usage Tracking Launch Proxy

## Overview

[Issue #42](https://github.com/ibm-granite-community/granite-cli/issues/42) asks
for a way to see how many tokens a launched agent actually burns against its
configured model, on the theory that one of granite-cli's selling points is
offloading tokens away from frontier agents. This plan adds an opt-in,
localhost-only reverse proxy that sits between the launched process and its
real model endpoint, purely to observe and total up token usage as responses
flow through — it does not alter requests or responses in any way, and it
imposes no overhead when the feature is off (the default).

This overturns the "no proxy" decision recorded in
`docs/specs/0000-initial-plan.md` (provider failover section: "Once tool is
launched, it communicates directly with the selected provider (no proxy)").
That decision was about provider *failover*, made before usage accounting was
a goal; it does not hold once tracking requests-in-flight is a stated
requirement. The proxy introduced here is strictly additive and turned off by
default, so it does not otherwise change the launch model described there.

**Design: intercept model construction at `ModelSource::from_config`, not a
`Capability` or `Launcher` change.** `Model::provider()` (default body in
`src/models/base.rs`) is the only place a `Model` produces connection
details — `base_url`/`api_key`/`verify_ssl` live on the `Provider` it
returns, not on `Model` itself. `ModelSource` (`src/models/mod.rs`) is the
single place any config-driven model comes into existence: it eagerly
constructs every model in `config.models`, and callers pull one out by id via
`ModelSource::take`. Usage tracking hooks in exactly there: when a
usage-tracking session is active (threaded through a new, non-persisted
`Config::usage_tracking` field, set once per `launch` invocation), `take()`
wraps the requested model in `UsageTrackingModel`
(`src/proxy/model_wrapper.rs`) before returning it. `UsageTrackingModel`
delegates every metadata method to the real model unchanged, and overrides
only `provider()` to return a `UsageTrackingProvider` that points
`base_url()` at a local `ProxyServer` (started synchronously inside `take()`)
and clears `api_key()` (the proxy holds the real credential and injects it
upstream; the launched process never sees it). Every other `Provider` method
delegates straight to the real provider.

This means `AgentModelCapability::bind()` — and any future capability that
resolves its model through `ModelSource` — needs zero tracking-specific code:
it already does exactly `self.model.provider()` →
`base_url()`/`api_key()`/`verify_ssl()`, so wrapping happens transparently
underneath it. Nothing about the `Capability` trait, `Launcher` trait, or any
concrete launcher changes. (An earlier iteration of this design wrapped
`Capability` itself via a decorator, `UsageTrackingCapability`, that
special-cased `Binding::AgentModel` in `bind()`; that coupled tracking to
`AgentModelCapability` specifically, since a future model-backed capability
would need its own case added to the wrapper. Wrapping at `ModelSource`
instead makes tracking a property of *how a model is obtained*, not of any
particular capability.)

**Unified streaming/non-streaming handling.** Anthropic and OpenAI report
usage over SSE (`data: {...}` lines); Ollama reports it as NDJSON, one JSON
object per line, with the final `done: true` line carrying the counts; a
non-streaming call is just a single JSON document with no line framing at
all. Rather than branching on content-type or `stream` flags, the proxy scans
every response through one pipeline (`scan_and_forward` in
`src/proxy/server.rs`): bytes are forwarded to the client unchanged as they
arrive, while a side buffer accumulates text and is scanned line-by-line for
recognizable SSE/NDJSON events. A non-streaming response never contains a
newline mid-body, so it simply never matches that framing and falls through
to `finalize_leftover`, which parses whatever is left in the buffer once the
stream ends as one JSON document. One code path serves both cases.

**Running totals, not deltas.** Anthropic's `message_delta` and Ollama's
final NDJSON line report *cumulative* token counts for the response so far,
not incremental deltas — summing every observed event would overcount.
`UsageStats::merge_max` accumulates by taking the elementwise max across
events within one response; only the `requests` counter (incremented once
per completed request by `UsageTracker::record`) is ever summed across
requests.

## Out of Scope (Future Work)

**MCP servers.** The issue asks for tracking "any/all model or MCP
endpoint," but granite-cli has no MCP configuration surface today, and MCP's
framing (JSON-RPC over stdio or SSE) is not a simple HTTP
request/response token-usage schema anyway. This plan covers models resolved
through `ModelSource` only. The `ProxyServer`/`UsageTracker` split in
`src/proxy/` is written generically enough that a future MCP-backed source
could plug into the same `tracker`/`servers` plumbing, but no MCP-specific
code is added now.

**Per-request / live usage display.** The summary is a single table printed
after the launched process exits (`ui.table` from `UsageTracker::snapshot()`
in `src/main.rs`). Streaming a running total to the terminal *during* the
session (e.g. a status bar) is not attempted here.

**Cost estimation.** Only raw token counts (input, output, cache write,
cache read) are tracked, per the issue's own scope ("keeping track of token
usage"). Mapping counts to a dollar cost would require a pricing table per
model/provider that does not exist anywhere in granite-cli today.

**Multiple capabilities sharing one model.** `ModelSource::take` removes and
wraps the model on first request, so each `model_id` is only ever wrapped
(and proxied) once per `launch` invocation, regardless of how many
capabilities are configured — no special handling is added or needed for
this case.

---

## Sub-Tasks

---

### Sub-Task 1 — Usage parsing and accumulation (`src/proxy/usage.rs`)

**Intent**
Isolate "how do I read token counts out of a provider's JSON" and "how do I
accumulate them correctly" from the HTTP/proxying mechanics, so the parsing
logic is unit-testable without spinning up a server.

**Expected Outcomes**
- `UsageStats { requests, input_tokens, output_tokens, cache_creation_tokens, cache_read_tokens: u64 }`,
  `Default`/`Copy`, with `add` (sum, used for combining separate completed
  requests) and `pub(crate) merge_max` (elementwise max, used within one
  streamed response).
- `UsageTracker`: `Mutex<HashMap<String, UsageStats>>` keyed by a caller-
  supplied label (the capability id); `record(label, delta)` adds one request
  plus `delta`'s fields; `snapshot()` returns a point-in-time clone.
- A single `pub fn parse_usage(value: &serde_json::Value) -> Option<UsageStats>`
  that sniffs the JSON shape rather than being told which provider produced
  it — no `ApiType` argument needed, since this layer doesn't have one to
  give it (see Sub-Task 2). It checks `.message.usage` (Anthropic
  `message_start`) or top-level `.usage` (Anthropic `message_delta` /
  OpenAI / any non-streaming body) first, trying Anthropic's key names
  (`input_tokens`/`output_tokens`/`cache_creation_input_tokens`/`cache_read_input_tokens`)
  then OpenAI's (`prompt_tokens`/`completion_tokens`/`prompt_tokens_details.cached_tokens`,
  the latter only present when the caller sets `stream_options.include_usage`) —
  the two key sets are disjoint, so trying both in turn is unambiguous. If
  neither matches, it falls through to Ollama's top-level
  `prompt_eval_count`/`eval_count` (present only on the line with
  `done: true`, or on a non-streaming body); Ollama never nests its counts
  under a `usage` key, so checking it last is safe. A non-matching shape
  returns `None` rather than an error.

**Relevant Context**
- Anthropic Messages API streaming events: `message_start` (usage nested
  under `message`), `message_delta` (usage at top level)
- OpenAI Chat Completions streaming: usage only on the final chunk, and only
  when requested
- Ollama `/api/chat` and `/api/generate`: NDJSON, `done: true` line carries
  `prompt_eval_count`/`eval_count`

**Status** — `[x] done`

---

### Sub-Task 2 — Reverse proxy server (`src/proxy/server.rs`)

**Intent**
Stand up a real localhost HTTP server per tracked model that forwards every
request/response byte-for-byte to the real upstream while feeding response
bytes through the Sub-Task 1 parser as they pass through, and injecting the
real API key so the launched process never has to hold it. Synchronous, so
it can be started from inside sync code (`ModelSource::take`, itself reached
from the sync, infallible `ConfigConstructable::new`) as long as a Tokio
runtime is already running somewhere up the call stack.

**Expected Outcomes**
- `ProxyServer::start(base_url, api_key, verify_ssl, tracker, label) ->
  anyhow::Result<Self>` — no longer `async`. Binds `127.0.0.1:0` via a plain
  OS `std::net::TcpListener::bind` call (OS-chosen ephemeral port — avoids
  port collisions across concurrent launches) + `set_nonblocking(true)`, then
  hands it to `tokio::net::TcpListener::from_std` and `tokio::spawn` — both
  ordinary sync calls that only need an *ambient* runtime, not an `.await`.
  Builds a `reqwest::Client` honoring `verify_ssl`, and spawns an `axum`
  server (`Router::new().fallback(any(proxy_handler))`) on a background
  task. `local_base_url` is exposed for the caller (`UsageTrackingModel::wrap`,
  Sub-Task 3) to point the wrapped provider at.
- `ProxyServer::shutdown(self)` signals graceful shutdown via a
  `oneshot::Sender` and awaits the server task's `JoinHandle`.
- `forward()` builds the outbound request to `base_url + <incoming
  path/query>`, strips hop-by-hop and auth headers
  (`is_forbidden_request_header`) before copying the rest, then — since this
  layer has no `ApiType` to pick a single header scheme with — injects the
  real key as both `x-api-key` and a bearer token; a real upstream only
  reads the header scheme it understands, so sending both is harmless.
  Response headers that describe framing we're about to replace
  (`content-length`, `transfer-encoding`, `connection`, `keep-alive`) are
  dropped (`is_forbidden_response_header`); everything else is copied
  through.
- `scan_and_forward` wraps `reqwest::Response::bytes_stream()` in a
  `futures_util::stream::unfold` that: forwards each chunk unchanged to the
  client; appends it to a `String` buffer; drains complete lines out of that
  buffer, feeding each to `scan_line` (SSE `data:` payload, or — Ollama only
  — a bare NDJSON object) which folds any parsed delta into a per-response
  running total via `merge_max`; and, once the upstream stream ends, parses
  whatever's left in the buffer as a single JSON document
  (`finalize_leftover`, covering the non-streaming case) before recording the
  accumulated total into `tracker` under `label` via one `record()` call.
- A parse miss anywhere in this pipeline (malformed line, unexpected shape)
  never fails the proxied response — the bytes still reach the client
  unchanged; only accounting is best-effort.

**Relevant Context**
- `src/registry/secret.rs`: `Secret` — masked in `Debug`, held only inside
  `ProxyServer`/`UpstreamState`, never forwarded to the client
- `axum` 0.8, added as a new direct dependency for the inbound listener (this
  was an explicit choice over building the same thing directly on `hyper`)
- `bytes`, added as a direct dependency so `bytes::Bytes` can be named
  explicitly in the stream's `Item` type

**Status** — `[x] done`

---

### Sub-Task 3 — Model wrapper (`src/proxy/model_wrapper.rs`) + `ModelSource::take` (`src/models/mod.rs`)

**Intent**
Make usage tracking a property of *how a model is obtained*, so no
`Capability` or `Launcher` implementation needs to know it exists, and any
future capability that resolves its model through `ModelSource` is
trackable for free.

**Expected Outcomes**
- `UsageTrackingContext { tracker: Arc<UsageTracker>, servers:
  Arc<Mutex<Vec<ProxyServer>>> }` — `Clone` (its fields are `Arc`), with a
  hand-written `Debug` (`ProxyServer` isn't `Debug`). Carried on
  `Config::usage_tracking` (Sub-Task 4) and cloned into every
  `UsageTrackingModel::wrap` call.
- `UsageTrackingModel { inner: Arc<dyn Model>, local_base_url: String }`.
  `UsageTrackingModel::wrap(inner, label, ctx) -> anyhow::Result<Self>` calls
  `inner.provider()?`, starts a `ProxyServer` for that provider's real
  `base_url`/`api_key`/`verify_ssl` (Sub-Task 2), pushes it into
  `ctx.servers`, and stores the proxy's `local_base_url`. Every `Model`
  method except `provider()` delegates straight to `inner`; `provider()`
  returns a `UsageTrackingProvider` wrapping the real provider.
- `UsageTrackingProvider { inner: Box<dyn Provider>, local_base_url: String }`
  — every `Provider` method except `base_url`/`api_key`/`verify_ssl`
  delegates to `inner`; `base_url()` returns `local_base_url`, `api_key()`
  returns `None` (the proxy holds the real credential), `verify_ssl()`
  returns `true` (the local proxy is plain `http`).
- `ModelSource` gains a `usage_tracking: Option<UsageTrackingContext>` field,
  set from `config.usage_tracking` in `from_config`, and a new
  `take(&mut self, model_id: &str) -> Option<Arc<dyn Model>>` that removes
  and returns the constructed model for `model_id`, wrapping it via
  `UsageTrackingModel::wrap` first when a tracking session is active. If the
  proxy fails to start, `take()` falls back to the untracked model with a
  logged warning rather than failing construction over an accounting
  feature. `AgentModelCapability::new` (Sub-Task 4) calls this instead of
  resolving a provider inline, which both makes it trackable and
  deduplicates the provider-resolution logic into one place.

**Relevant Context**
- `src/models/base.rs`: `Model::provider()`'s default body — the only place
  a `Model` produces connection details, and therefore the only method
  `UsageTrackingModel` needs to override
- `src/capabilities/agent_model.rs`: `AgentModelCapability::bind()` — already
  does exactly `self.model.provider()` →
  `base_url()`/`api_key()`/`verify_ssl()`, so wrapping at this layer requires
  no change there at all

**Status** — `[x] done`

---

### Sub-Task 4 — Wire `-u`/`--usage-tracking` into `granite-cli launch`

**Intent**
Expose the feature as a single opt-in flag on `launch`, off by default per
the issue, and surface what was tracked once the launched process exits.

**Expected Outcomes**
- `LaunchWithOutput` (`src/main.rs`) gains `#[arg(short = 'u', long =
  "usage-tracking")] usage_tracking: bool`.
- `run_launch` takes `usage_tracking: bool`. Tracking is actually enabled
  only when `usage_tracking && !dry_run` — under `--dry-run` there is no
  subprocess to point a proxy at, and showing the real upstream URL in the
  dry-run overlay is more informative than a not-yet-running proxy URL.
- When enabled, `config.usage_tracking` is set to a
  `proxy::UsageTrackingContext { tracker: Arc::clone(&tracker), servers:
  Arc::clone(&proxy_servers) }` once, before any capability or the launcher
  is constructed. Nothing else about the capability-construction loop
  changes — no per-capability branching or wrapping call at the
  `bind_capability` site; any capability whose model comes through
  `ModelSource::take` (Sub-Task 3) is tracked transparently.
- After `launcher.launch(...)` returns (regardless of whether the launched
  process's own exit status was successful), every started `ProxyServer` is
  drained from `proxy_servers` and shut down, and `print_usage_summary`
  renders a `ui.table` of per-capability totals plus a `Total` row — skipped
  entirely if `tracker.snapshot()` is empty (nothing was ever proxied, or the
  launched process made no requests).
- `anyhow::bail!` on a non-zero exit status still happens, but *after* the
  usage summary is printed — a failed run's usage is still worth seeing.

**Relevant Context**
- `src/main.rs`: `LaunchWithOutput`, `run_launch`, `print_usage_summary`
- `src/utils/ui/base.rs`: `Ui::table(title, headers, rows)`

**Status** — `[x] done`

---

### Sub-Task 5 — Unblock the async proxy while the subprocess runs

**Intent**
`run_command` (`src/launchers/base.rs`) previously called
`cmd.spawn()?.wait()?` directly on the async task running `run_launch`.
`Child::wait()` (the `std::process` kind) blocks the OS thread until the
child exits; on a single-threaded-per-task view that starves the Tokio
runtime of the chance to poll the proxy server's connections concurrently.

**Expected Outcomes**
- The `spawn`+`wait` pair moves into `tokio::task::spawn_blocking`, so the
  blocking wait happens on a dedicated blocking-pool thread instead of an
  async worker thread, letting the proxy's `axum` server keep serving
  requests on the runtime for the whole lifetime of the launched process.
- No behavior change for the non-tracking path — this affects every launch,
  but `spawn_blocking` + `.await` on its `JoinHandle` is otherwise
  functionally identical to calling `wait()` inline.

**Relevant Context**
- `src/launchers/base.rs`: `run_command`, called from the default
  `Launcher::launch` and shared by all concrete launchers

**Status** — `[x] done`

---

### Sub-Task 6 — Tests

**Intent**
Cover parsing correctness, accumulation semantics, end-to-end proxying, and
the model-wrapper rewrite — without needing a real provider.

**Expected Outcomes**
- `src/proxy/usage.rs`: `parse_usage` tests covering every shape (Anthropic
  streaming + non-streaming, OpenAI with cached tokens, Ollama done/non-done
  lines), `merge_max` vs `add` semantics, `UsageTracker::record`/`snapshot`
  accumulation across labels.
- `src/proxy/server.rs`: `scan_line` recognizes SSE and NDJSON framing and
  ignores the `[DONE]` sentinel; `finalize_leftover` parses a full
  non-streaming body; an end-to-end `scan_and_forward` test feeding fake SSE
  chunks through `futures_util::stream::iter` and asserting the tracker
  records exactly one request with the right totals; forbidden-header
  filtering in both directions.
- `src/proxy/model_wrapper.rs`: `FakeModel`/`FakeProvider` test doubles
  confirm `UsageTrackingModel::wrap` starts a real `ProxyServer` (bound to an
  actual ephemeral port) and the wrapped model's `provider().base_url()`
  points at it rather than the real upstream, with `api_key()` cleared to
  `None`; metadata methods (`family`, `context_length`, etc.) still return
  the inner model's real values unchanged.
- `src/models/mod.rs`: `ModelSource::take` tests — returns `Some` for a
  configured model id and removes it (a second `take` of the same id returns
  `None`), returns `None` for an unknown id, and — with `usage_tracking` set
  to a real context — returns a model whose `provider().base_url()` points
  at `127.0.0.1` rather than the configured upstream.

**Relevant Context**
- `futures_util::stream::iter`, `stream::unfold` — used to drive fake
  streaming responses in tests without a real socket

**Status** — `[x] done`
