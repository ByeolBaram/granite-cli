# Structural Refactor: Remove `models` argument from `Capability::bind` and `Launcher::bind_capability`

## Problem

Currently, `AgentModelCapability.bind` takes a `models: &(dyn Configured<dyn Model> + Sync)` argument and looks up the real model instance from `models.instances()` by `model_id` (from `config.model_id`). This pattern leaks the concrete `Configured<dyn Model>` type through the abstract `Capability` and `Launcher` trait interfaces, which is an anti-pattern.

The `models` argument is needed at `bind` time because the model instance lookup happens there. Instead, this should happen at construction time for `AgentModelCapability` so the internal data holds the real model instance, and `bind` no longer needs to look up from `models`.

## Goal

Refactor so that `AgentModelCapability` stores its model instance at construction time, and `Capability::bind` no longer takes the `models` argument. Consequently, `Launcher::bind_capability` also no longer needs the `models` argument.

## Key Constraint from user

`AgentModelCapability` should always set the `model` field (not `Option`) in `new` and `from_config`, because the capability cannot be constructed without a valid model and still be useful. Thus the `model` field should not be `Option` — it should be required.

## Current signatures

```rust
// src/capabilities/base.rs
async fn bind(
    &self,
    request: BindingRequest,
    models: &(dyn Configured<dyn Model> + Sync),
) -> anyhow::Result<Binding>;

// src/launchers/base.rs
async fn bind_capability(
    &mut self,
    capability: &dyn crate::capabilities::Capability,
    models: &(dyn crate::dependency::Configured<dyn crate::models::Model> + Sync),
) -> anyhow::Result<()>;
```

## Current `AgentModelCapability`

```rust
pub struct AgentModelCapability {
    config: AgentModelCapabilityConfig, // contains model_id
}

impl ConfigConstructable for AgentModelCapability {
    fn new(cfg: &serde_json::Value) -> Self {
        let config: AgentModelCapabilityConfig = serde_json::from_value(cfg.clone()).unwrap_or_default();
        Self { config }
    }
}

async fn bind(
    &self,
    request: BindingRequest,
    models: &(dyn Configured<dyn Model> + Sync),
) -> anyhow::Result<Binding> {
    let api_type = match request {
        BindingRequest::AgentModel(AgentModelBindingRequest { api_type }) => api_type,
        #[allow(unreachable_patterns)]
        other => anyhow::bail!("AgentModelCapability does not handle {:?}", other),
    };
    let model_id = self.config.model_id.clone();

    let (_, model) = models
        .instances()
        .into_iter()
        .find(|(id, _)| id == &model_id)
        .ok_or_else(|| anyhow::anyhow!("model '{model_id}' not configured"))?;

    // ... rest uses model and provider to construct Binding::AgentModel
}
```

## Proposed changes

1. **Add `model: Box<dyn Model>` field to `AgentModelCapability`** (required, not `Option`). In `new`, initialize `model` — `from_config` will set it.

2. **Modify `CapabilitySource::from_config`** to also construct and store the model alongside each `AgentModelCapability`. After constructing the `AgentModelCapability` via the factory, look up the corresponding model from `ModelSource` using `model_id` from the config and set it on the capability.

3. **Change `Capability::bind` signature** from:
   ```rust
   async fn bind(
       &self,
       request: BindingRequest,
       models: &(dyn Configured<dyn Model> + Sync),
   ) -> anyhow::Result<Binding>;
   ```
   to:
   ```rust
   async fn bind(
       &self,
       request: BindingRequest,
   ) -> anyhow::Result<Binding>;
   ```
   (remove `models` argument)

4. **Change `Launcher::bind_capability` signature** from:
   ```rust
   async fn bind_capability(
       &mut self,
       capability: &dyn crate::capabilities::Capability,
       models: &(dyn crate::dependency::Configured<dyn crate::models::Model> + Sync),
   ) -> anyhow::Result<()>;
   ```
   to:
   ```rust
   async fn bind_capability(
       &mut self,
       capability: &dyn crate::capabilities::Capability,
   ) -> anyhow::Result<()>;
   ```
   (remove `models` argument)

5. **Update `AgentModelCapability::bind`** to use `self.model` directly instead of looking up from `models`. No longer needs the `models` argument.

6. **Update launcher implementations** (`ClaudeLauncher::bind_capability` and `FakeLauncher::bind_capability`) to call `capability.bind(request)` without passing `models`.

7. **Update tests** — the test doubles in `agent_model.rs` use `FakeSource<dyn Model>` which already implements `Configured<dyn Model>`. `CapabilitySource::from_config` will work with it.

## Implementation details

- `AgentModelCapability` struct:
  ```rust
  pub struct AgentModelCapability {
      config: AgentModelCapabilityConfig,
      model: Box<dyn Model>,  // required field, always set
  }
  ```

- `ConfigConstructable` impl for `AgentModelCapability`'s `new` should only take `config` and not the model — the model will be set by `CapabilitySource::from_config`. So `new` remains simple.

- `CapabilitySource::from_config`:
  After constructing each capability via the factory (including `AgentModelCapability`), look up the model from `ModelSource` using the `model_id` from the config and set it on the capability.

- `AgentModelCapability::bind` will use `self.model` directly (no `models` lookup).

- `FakeSource` in tests already works similarly — tests should continue to work.

## Files to modify (in order)

1. `src/capabilities/agent_model.rs` — add `model: Box<dyn Model>` field to `AgentModelCapability`, modify `bind` to use `self.model`.
2. `src/capabilities/mod.rs` — modify `CapabilitySource::from_config` to look up and set the model on `AgentModelCapability` instances.
3. `src/capabilities/base.rs` — change `Capability` trait's `bind` signature (remove `models` arg).
4. `src/launchers/base.rs` — change `Launcher` trait's `bind_capability` signature (remove `models` arg).
5. `src/launchers/claude.rs` — update `bind_capability` to call `capability.bind(request)` without `models`.
6. `src/launchers/base.rs` — update `FakeLauncher::bind_capability` similarly.
7. `src/commands/capability.rs` — no changes needed.
8. `src/commands/launcher.rs` — no changes needed.

## Validation

- All existing tests should pass.
- `AgentModelCapability` cannot be constructed without a valid model — this satisfies the constraint that `model` is always set (not `Option`).
- `bind` no longer needs to lookup the model from `models`; it uses `self.model` directly.
- `bind_capability` no longer passes `models` to `capability.bind`.
