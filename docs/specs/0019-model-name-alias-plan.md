# Model Name Alias Plan

## Overview

When a model's catalog ID (e.g. `granite-4.1-8b`) differs from the name the
provider requires at runtime (e.g. `granite4.1:8b` for Ollama), the
`AgentModelBinding.model_name` currently carries the wrong value and the
launch fails.

The fix adds a `model_alias(model, variant)` method to the `Provider` trait.
Each provider implementation encodes its own naming convention: Ollama derives
the alias from its variant URL (`https://ollama.com/library/granite4.1:8b` →
`granite4.1:8b`); other providers return `None` (meaning "use the catalog
ID"). `AgentModelCapability` is updated to resolve the configured variant and
pass it to the provider, which then determines the correct name to emit into
`AgentModelBinding.model_name`.

### Key design decisions

- **Generic mechanism**: The alias is computed by the provider from the variant
  URL — no per-model hardcoding anywhere.
- **Variant passed explicitly**: `ModelConfig.variant` is already stored but
  not threaded to the `Model` trait. `AgentModelCapability` carries it and
  resolves the `ModelVariant` at bind time (the same pattern used in
  `commands/model.rs` for pull).
- **Default is `None`**: The `Provider` trait default implementation returns
  `None`, so providers that do not need an alias (OpenAI-compatible, LM Studio,
  llamacpp) require no changes beyond a test.
- **`AgentModelBinding.model_name` always carries the provider-ready name**:
  callers (launchers) remain unchanged.

---

## Sub-Tasks

---

### Sub-Task 1 — Add `model_alias` to the `Provider` trait

**Intent**
Encode the provider-specific naming convention at the `Provider` trait level.
Each provider knows how its server expects models to be identified; the trait
method makes this explicit and overridable. The default returns `None`,
meaning "use the catalog ID".

**Expected Outcomes**
- `Provider` trait gains `fn model_alias(&self, model: &ModelMetadata, variant: Option<&ModelVariant>) -> Option<String>` with a default implementation returning `None`.
- All existing provider implementations compile without modification (they
  inherit the default).

**Todo List**
1. In `src/providers/base.rs`, add the method to the `Provider` trait with a
   default body of `None`:
   ```rust
   fn model_alias(
       &self,
       _model: &ModelMetadata,
       _variant: Option<&ModelVariant>,
   ) -> Option<String> {
       None
   }
   ```
2. Import `ModelMetadata` and `ModelVariant` in the trait file if not already
   present (they live in `crate::models`).
3. Update the `FakeProvider` test double in
   `src/capabilities/agent_model.rs` to inherit the default (no explicit impl
   needed since the default is `None`).

**Relevant Context**
- `Provider` trait defined in [`src/providers/base.rs:124`](src/providers/base.rs:124)
- `ModelMetadata` and `ModelVariant` from `crate::models`
- `FakeProvider` test double at
  [`src/capabilities/agent_model.rs:218`](src/capabilities/agent_model.rs:218)

**Status**: [ ] pending

---

### Sub-Task 2 — Implement `model_alias` in `OllamaProvider`

**Intent**
Ollama uses names like `granite4.1:8b` rather than the catalog ID
`granite-4.1-8b`. The Ollama variant URL already encodes this name
(`https://ollama.com/library/granite4.1:8b`). The private `ollama_library_ref`
helper already knows how to extract it — `model_alias` should reuse that logic.

**Expected Outcomes**
- `OllamaProvider::model_alias` returns `Some("granite4.1:8b")` when the
  model has an Ollama-format variant selected.
- Returns `None` when the selected variant has no Ollama library URL (e.g. the
  selected variant is GGUF from HuggingFace, not the Ollama variant).

**Todo List**
1. In `src/providers/ollama.rs`, add an `impl Provider for OllamaProvider`
   override of `model_alias`:
   ```rust
   fn model_alias(
       &self,
       _model: &ModelMetadata,
       variant: Option<&ModelVariant>,
   ) -> Option<String> {
       variant
           .and_then(|v| ollama_library_ref(&v.url))
           .map(str::to_string)
   }
   ```
2. Add unit tests covering:
   - Ollama variant URL → correct alias extracted.
   - Non-Ollama variant URL (GGUF HuggingFace) → `None`.
   - `variant` is `None` → `None`.

**Relevant Context**
- `ollama_library_ref` private helper at
  [`src/providers/ollama.rs:81`](src/providers/ollama.rs:81)
- Ollama variant example URL: `"https://ollama.com/library/granite4.1:8b"`
- Existing tests for `ollama_library_ref` at
  [`src/providers/ollama.rs:419`](src/providers/ollama.rs:419)

**Status**: [ ] pending

---

### Sub-Task 3 — Thread the configured variant into `AgentModelCapability`

**Intent**
`AgentModelCapability` needs to know which `ModelVariant` the user configured
so it can pass it to `provider.model_alias()` at bind time. The `variant`
string is already on `ModelConfig`; it needs to be stored on the capability
and resolved to a `&ModelVariant` at bind time using the same
`"format/precision"` split pattern used in `commands/model.rs`.

**Expected Outcomes**
- `AgentModelCapability` stores an `Option<String>` named `configured_variant`
  (the raw `"format/precision"` string from `ModelConfig`).
- In `with_config`, `model_cfg.variant.clone()` is captured into
  `configured_variant`.
- In the `new()` path, `configured_variant` is `None`.
- A helper method `resolve_variant<'a>(&self, model: &'a dyn Model) -> Option<&'a ModelVariant>`
  parses `configured_variant` (split on `/`, match format + precision from
  `model.variants()`) — exactly the same lookup used in model pull.

**Todo List**
1. Add `configured_variant: Option<String>` field to `AgentModelCapability`.
2. In `with_config`, set `configured_variant: model_cfg.variant.clone()`.
3. In `new()`, set `configured_variant: None`.
4. Add `fn resolve_variant<'a>(&self, model: &'a dyn Model) -> Option<&'a ModelVariant>`
   that splits `self.configured_variant` on `'/'` and finds the matching
   `ModelVariant` in `model.variants()`.

**Relevant Context**
- `AgentModelCapability` struct at
  [`src/capabilities/agent_model.rs:29`](src/capabilities/agent_model.rs:29)
- `ModelConfig.variant` format established by `commands/model.rs:527-530` as
  `"{format}/{precision}"`.
- Existing variant lookup pattern in `src/commands/model.rs` around line
  634-642 for reference.

**Status**: [ ] pending

---

### Sub-Task 4 — Use `provider.model_alias()` in `AgentModelBinding`

**Intent**
Wire together the new pieces: at bind time, resolve the configured variant,
ask the provider for its preferred name, and emit that into
`AgentModelBinding.model_name`. Fall back to the catalog ID when the provider
returns `None`.

**Expected Outcomes**
- `AgentModelBinding.model_name` is `"granite4.1:8b"` when binding
  `granite-4.1-8b` against Ollama with the Ollama variant configured.
- `AgentModelBinding.model_name` remains the catalog ID for providers that
  return `None` from `model_alias`.
- The existing test `bind_succeeds_for_matching_provider_and_model` continues
  to pass unchanged (its `FakeProvider` returns `None`, so the catalog ID is
  used).

**Todo List**
1. In `AgentModelCapability::bind()`, after constructing the `provider`:
   ```rust
   let resolved_variant = self.resolve_variant(self.model.as_ref());
   let effective_model_name = provider
       .model_alias(self.model.metadata_ref(), resolved_variant)
       .unwrap_or_else(|| model_id.to_string());
   ```
   Replace `model_name: model_id.to_string()` with
   `model_name: effective_model_name`.
2. `model.metadata_ref()` — if `Model` does not expose a `ModelMetadata`
   reference, pass the individual fields that `model_alias` needs (family,
   variants). Prefer the simplest approach: since `model_alias` currently only
   uses `variant` and ignores `_model`, and the `ModelMetadata` is not cheaply
   available on the `Model` trait, pass `self.model.variants()` instead and
   adjust the signature to `model_alias(&self, variants: &[ModelVariant], variant: Option<&ModelVariant>)`.
   - **Reconsider the signature**: given that `model_alias` currently ignores
     the model arg (only uses the variant URL), keep the signature simple:
     `fn model_alias(&self, variant: Option<&ModelVariant>) -> Option<String>`.
     This avoids the question of how to get `ModelMetadata` from `Model`.
     Update Sub-Task 1 and 2 to match.

**Relevant Context**
- `bind()` in [`src/capabilities/agent_model.rs:122-174`](src/capabilities/agent_model.rs:122)
- `AgentModelBinding.model_name` flows to launchers via `env_overlay()`, e.g.
  `ANTHROPIC_MODEL` in [`src/launchers/claude.rs:89`](src/launchers/claude.rs:89)

**Status**: [ ] pending

---

### Sub-Task 5 — Tests for the full alias flow

**Intent**
Cover the new alias path end-to-end in the `AgentModelCapability` test module:
when the configured variant corresponds to an Ollama URL, the binding carries
the Ollama name.

**Expected Outcomes**
- `bind_uses_provider_alias_for_ollama_variant`: configured variant is
  `"Ollama/Q4_K_M"`, provider is `FakeProvider` with `model_alias` overridden
  to return `Some("granite4.1:8b")`, binding has
  `model_name == "granite4.1:8b"`.
- `bind_falls_back_to_catalog_id_when_alias_is_none`: configured variant
  present but `FakeProvider.model_alias` returns `None`, binding has
  `model_name == "granite-3.1-8b-instruct"`.

**Todo List**
1. Add `model_alias` method to `FakeProvider` that returns a configurable
   `Option<String>` (add field `alias: Option<String>` to `FakeProvider`).
2. Add the two test cases above using the existing `capability_with_test_model`
   helper pattern.

**Relevant Context**
- Test module in [`src/capabilities/agent_model.rs:199`](src/capabilities/agent_model.rs:199)
- `FakeProvider` at [`src/capabilities/agent_model.rs:218`](src/capabilities/agent_model.rs:218)

**Status**: [ ] pending
