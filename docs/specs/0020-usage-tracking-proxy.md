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

**Design: decorator over `Capability`, not a `Launcher` change.** The
existing binding flow is `Capability::bind(request) -> Binding`, and
`AgentModelBinding` already carries everything a launcher needs to talk to a
model (`base_url`, `api_key`, `api_type`, ...). Usage tracking is implemented
as `UsageTrackingCapability`, a `Capability` decorator that wraps any other
capability, calls through to it in `bind()`, and — only when the result is a
`Binding::AgentModel` — starts a local `ProxyServer` for that binding's
upstream and rewrites the returned binding to point at `127.0.0.1:<ephemeral
port>` instead, with `api_key` cleared (the proxy holds the real credential
and injects it upstream; the launched process never sees it). Every other
`Capability` method delegates straight to the inner capability. This means
`run_launch` decides whether to wrap, and nothing in `Launcher`,
`ClaudeLauncher`, `BobLauncher`, or the `Capability` trait itself needs to
know tracking exists.

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
endpoint," but granite-cli has no MCP configuration surface today — there is
no existing `Capability`/`Binding` shape to decorate, and inventing one from
scratch is a separate, larger design problem (MCP's framing is JSON-RPC over
stdio or SSE, not a simple HTTP request/response token-usage schema). This
plan covers `Binding::AgentModel` only. The `UsageTrackingCapability`
decorator pattern and the `ProxyServer`/`UsageTracker` split in `src/proxy/`
are written generically enough that a future MCP binding type could plug into
the same `tracker`/`servers` plumbing, but no MCP-specific code is added now.

**Per-request / live usage display.** The summary is a single table printed
after the launched process exits (`ui.table` from `UsageTracker::snapshot()`
in `src/main.rs`). Streaming a running total to the terminal *during* the
session (e.g. a status bar) is not attempted here.

**Cost estimation.** Only raw token counts (input, output, cache write,
cache read) are tracked, per the issue's own scope ("keeping track of token
usage"). Mapping counts to a dollar cost would require a pricing table per
model/provider that does not exist anywhere in granite-cli today.

**Multiple `AgentModel` capabilities racing for one proxy.** If a launcher is
ever configured with more than one enabled `AgentModel`-binding capability
(already an unenforced edge case per `docs/specs/0018-capability-binding-plan.md`),
usage tracking simply starts one `ProxyServer` per `bind()` call and records
each under its own capability-id label — no special handling is added or
needed here, but no attempt is made to detect or warn about the underlying
over-selection either.

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
- Per-`ApiType` parsing functions, each returning `Option<UsageStats>` so a
  non-matching shape is silently skipped rather than treated as an error:
  `parse_json_body` (whole-body, non-streaming), `parse_sse_event` (one
  decoded SSE payload), `parse_ndjson_line` (one decoded NDJSON line).
  Anthropic checks `.message.usage` (on `message_start`) and top-level
  `.usage` (on `message_delta`); OpenAI reads `.usage.{prompt,completion}_tokens`
  and `.usage.prompt_tokens_details.cached_tokens` (only present when the
  caller sets `stream_options.include_usage`); Ollama reads top-level
  `prompt_eval_count`/`eval_count`, present only on the line with `done: true`.

**Relevant Context**
- `src/providers/base.rs`: `ApiType` enum (`OpenAI`, `Ollama`, `Anthropic`)
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
Stand up a real localhost HTTP server per `AgentModelBinding` that forwards
every request/response byte-for-byte to the real upstream while feeding
response bytes through the Sub-Task 1 parsers as they pass through, and
injecting the real API key so the launched process never has to hold it.

**Expected Outcomes**
- `ProxyServer::start(binding, tracker, label)` binds `127.0.0.1:0` (OS-chosen
  ephemeral port — avoids port collisions across concurrent launches), builds
  a `reqwest::Client` honoring `binding.verify_ssl`, and spawns an `axum`
  server (`Router::new().fallback(any(proxy_handler))`) on a background
  task. `local_base_url` is exposed for the caller to substitute into the
  rewritten binding.
- `ProxyServer::shutdown(self)` signals graceful shutdown via a
  `oneshot::Sender` and awaits the server task's `JoinHandle`.
- `forward()` builds the outbound request to `binding.base_url +
  <incoming path/query>`, strips hop-by-hop and auth headers
  (`is_forbidden_request_header`) before copying the rest, then injects the
  real key as `x-api-key` (Anthropic) or a bearer token (OpenAI/Ollama).
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
- `src/capabilities/base.rs`: `AgentModelBinding` fields consumed here
  (`base_url`, `api_key`, `api_type`, `verify_ssl`)
- `src/registry/secret.rs`: `Secret` — masked in `Debug`, held only inside
  `ProxyServer`/`UpstreamState`, never forwarded to the client
- `axum` 0.8, added as a new direct dependency for the inbound listener (this
  was an explicit choice over building the same thing directly on `hyper`)
- `bytes`, added as a direct dependency so `bytes::Bytes` can be named
  explicitly in the stream's `Item` type

**Status** — `[x] done`

---

### Sub-Task 3 — `Capability` decorator (`src/proxy/capability_wrapper.rs`)

**Intent**
Make usage tracking a property of *how a capability is bound*, so no
`Launcher` implementation needs to know it exists.

**Expected Outcomes**
- `UsageTrackingCapability { inner: Box<dyn Capability>, label: String,
  tracker: Arc<UsageTracker>, servers: Arc<Mutex<Vec<ProxyServer>>> }`.
- Every `Capability` method except `bind` delegates straight to `inner`.
- `bind()` calls `inner.bind(request)`, and if the result is
  `Binding::AgentModel(agent_model)`, starts a `ProxyServer` for it, pushes
  the server into `servers` (so the caller can drain and shut all of them
  down later), and returns a rewritten `Binding::AgentModel` with `base_url`
  pointed at the proxy's `local_base_url` and `api_key` cleared.

**Relevant Context**
- `src/capabilities/base.rs`: `Capability` trait, `Binding`,
  `AgentModelBinding`
- Binding today has exactly one variant (`AgentModel`) since
  `docs/specs/0018-capability-binding-plan.md` — the `match` in `bind()` is
  written to make adding a future variant (e.g. an MCP binding) a compile
  error at the match, not a silent no-op

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
- When enabled, each capability is wrapped in `UsageTrackingCapability::new(capability,
  cap_id.clone(), Arc::clone(&tracker), Arc::clone(&proxy_servers))` before
  `bind_capability` is called, instead of passing the raw capability through.
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
the capability-decorator rewrite — without needing a real provider.

**Expected Outcomes**
- `src/proxy/usage.rs`: per-`ApiType` parse tests (Anthropic streaming +
  non-streaming, OpenAI with cached tokens, Ollama done/non-done lines),
  `merge_max` vs `add` semantics, `UsageTracker::record`/`snapshot`
  accumulation across labels.
- `src/proxy/server.rs`: `scan_line` recognizes SSE and NDJSON framing and
  ignores the `[DONE]` sentinel; `finalize_leftover` parses a full
  non-streaming body; an end-to-end `scan_and_forward` test feeding fake SSE
  chunks through `futures_util::stream::iter` and asserting the tracker
  records exactly one request with the right totals; forbidden-header
  filtering in both directions.
- `src/proxy/capability_wrapper.rs`: a `FakeCapability` test double confirms
  `bind()` starts a real `ProxyServer` (bound to an actual ephemeral port),
  rewrites `base_url` to it, and clears `api_key`; a second test confirms
  every non-`bind` method still delegates to `inner`.

**Relevant Context**
- `futures_util::stream::iter`, `stream::unfold` — used to drive fake
  streaming responses in tests without a real socket

**Status** — `[x] done`
