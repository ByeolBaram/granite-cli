# Spec 0001: Config Lifecycle — AppContext, Registry/Configured Separation

## Problem

1. `Config::load()` reads only `config.yaml` and never populates its HashMaps from individual component YAML files (`models/{id}.yaml`, `capabilities/{id}.yaml`, etc.).
2. `save_model()` and similar methods write individual files, but nothing reads them back.
3. `model list`, `capability list`, and similar commands read only from static registries — configured items are invisible.
4. The `Config` HashMaps (`models`, `providers`, `capabilities`, `tools`) exist but are never populated at runtime.

## Design Decisions

### AppContext over Global Singleton

A global `LazyLock<Mutex<Config>>` would avoid repeated disk reads but:
- Breaks test isolation (tests can't create independent configs)
- Introduces `Mutex` overhead in a single-threaded CLI context
- Makes mutation visibility implicit at call sites

**Chosen approach:** `AppContext` is constructed once in `main()`, loaded from disk, then passed down the call chain.

### Rust Const-Correctness via Borrowing

Rust's type system enforces const-correctness at the borrow level:

- `&Config` — caller can only invoke `&self` methods. Compiler prevents mutation.
- `&mut Config` — caller can invoke both `&self` and `&mut self` methods.

A command that doesn't need mutation takes `&Config` and gets no mutators at its call site. A command that does need mutation takes `&mut Config` and can down-scope to `&Config` when calling sub-functions that don't mutate. The compiler enforces this at every call site — stronger than C++ const-correctness.

### Registry vs. Configured — Two Separate Views

```
Registry     → Bundled, static, read-only. Loaded from code.
Config       → User-created, persistent, mutable. Loaded from disk.
```

The `Config` HashMaps hold configured items keyed by ID. An item can exist in both:
- `BUNDLED` — available in the static registry but not yet set up by the user
- `CONFIGURED` — the user has run setup and it's persisted

If both, show once with both indicators.

### Implicit Persistence via AppContext Lifecycle

- `AppContext` is constructed once — loads everything from disk into memory.
- `Config` mutation methods (`insert`, `remove`, `update`) modify HashMaps and persist to disk automatically.
- Read-only commands receive `&Config` — no disk writes.
- No explicit `load()` or `save()` calls scattered across commands.
- Individual component YAML files (`models/{id}.yaml`, etc.) are the canonical storage. `config.yaml` is loaded for backward compatibility on first construct but not re-written.

## Implementation

### 1. `src/config/mod.rs` — Rewrite Config lifecycle

Remove: `Config::load()`, `Config::save()`, all `load_*()` / `save_*()` methods.

Add:

```rust
pub struct Config {
    pub models: HashMap<String, ModelConfig>,
    pub providers: HashMap<String, ProviderConfig>,
    pub capabilities: HashMap<String, CapabilityConfig>,
    pub routing: RoutingConfig,
    pub shell: ShellConfig,
    pub tools: HashMap<String, ToolConfig>,
}

impl Config {
    // Constructor — loads from disk, creates directories if needed
    pub fn new() -> Result<Self>;

    // Persist all HashMaps to individual YAML files
    fn save(&self) -> Result<()>;

    // Mutation methods — modify HashMap + auto-save
    pub fn insert_model(&mut self, id: &str, config: ModelConfig);
    pub fn remove_model(&mut self, id: &str);
    pub fn update_model(&mut self, id: &str, f: impl FnOnce(&mut ModelConfig));
    pub fn get_model(&self, id: &str) -> Option<&ModelConfig>;

    // Same pattern for provider, capability, tool
    pub fn insert_provider(&mut self, id: &str, config: ProviderConfig);
    pub fn remove_provider(&mut self, id: &str);
    pub fn update_provider(&mut self, id: &str, f: impl FnOnce(&mut ProviderConfig));
    pub fn get_provider(&self, id: &str) -> Option<&ProviderConfig>;

    pub fn insert_capability(&mut self, id: &str, config: CapabilityConfig);
    pub fn remove_capability(&mut self, id: &str);
    pub fn update_capability(&mut self, id: &str, f: impl FnOnce(&mut CapabilityConfig));
    pub fn get_capability(&self, id: &str) -> Option<&CapabilityConfig>;

    pub fn insert_tool(&mut self, id: &str, config: ToolConfig);
    pub fn remove_tool(&mut self, id: &str);
    pub fn update_tool(&mut self, id: &str, f: impl FnOnce(&mut ToolConfig));
    pub fn get_tool(&self, id: &str) -> Option<&ToolConfig>;
}
```

`Config::new()` loading logic:
1. Call `ensure_directories()` to create config dir + subdirectories if missing
2. Read `config.yaml` (if exists) — for backward compatibility, populate HashMaps from it
3. Iterate each component subdirectory (`models/`, `providers/`, `capabilities/`, `tools/`)
4. Load every `*.yaml` file into the corresponding HashMap (overrides `config.yaml` data)
5. Return the populated Config

`Config::save()` logic:
1. For each entry in each HashMap, write to `components_dir/{id}.yaml`
2. (No `config.yaml` rewrite needed — component files are canonical)

### 2. `src/main.rs` — Add AppContext and wire into command dispatch

```rust
struct AppContext {
    config: Config,
}

impl AppContext {
    fn new() -> Result<Self> {
        Ok(Self {
            config: Config::new()?,
        })
    }
}
```

Update `main()` to create `AppContext` once, then pass it through command functions. Change command function signatures:

```rust
// Read-only commands: &AppContext (or just &Config)
async fn run_model_list(ctx: &AppContext, filter: Option<ModelType>) -> Result<()>

// Mutating commands: &mut AppContext
async fn run_model_setup(ctx: &mut AppContext, model_id: &str) -> Result<()>
```

### 3. `src/commands/model.rs` — `model list` merges bundled + configured

```rust
pub fn list(ctx: &AppContext, filter_type: Option<ModelType>) -> Result<()> {
    let registry = &*registry::MODEL_REGISTRY;
    let models = registry.list();

    // Print header with columns: ID, FAMILY, SIZE, CONTEXT, TYPE, STATUS
    // STATUS: BUNDLED, CONFIGURED, or BUNDLED, CONFIGURED

    // Iterate bundled models from registry
    // For each, check if it has a configured entry in ctx.config.models
    // Display with appropriate status indicators

    // Then list any configured models NOT in the registry (user-defined extras)
    // These would show with only CONFIGURED status
}
```

### 4. `src/commands/model.rs` — `model info` shows configuration state

```rust
pub fn info(ctx: &AppContext, model_id: &str) -> Result<()> {
    let registry = &*registry::MODEL_REGISTRY;

    // Show registry metadata (same as before)

    // Check ctx.config.models for configured entry
    if let Some(configured) = ctx.config.get_model(model_id) {
        println!("\nConfiguration:");
        println!("  Provider: {:?}", configured.provider_id);
        println!("  Variant: {:?}", configured.variant);
        println!("  Enabled: {}", configured.enabled);
        println!("  API Key: {}", masked(configured.api_key.as_deref()));
    }
}
```

### 5. `src/commands/model.rs` — `model setup` uses mutation methods

```rust
pub fn setup(ctx: &mut AppContext, model_id: &str) -> Result<()> {
    // ... existing wizard logic ...

    let model_config = ModelConfig { ... };

    ctx.config.insert_model(model_id, model_config);

    println!("Model '{}' configured successfully!", model.id);
    Ok(())
}
```

### 6. `src/commands/capability.rs` — Same pattern

`capability list`:
- List bundled from `CAPABILITY_REGISTRY` with `BUNDLED` indicator
- List configured from `ctx.config.capabilities` with `CONFIGURED` indicator
- Merge if an ID exists in both

`capability setup`:
```rust
pub fn setup(ctx: &mut AppContext, capability_id: &str) -> Result<()> {
    // ... existing setup logic ...

    let capability_config = CapabilityConfig { ... };
    ctx.config.insert_capability(capability_id, capability_config);

    Ok(())
}
```

### 7. `src/commands/mod.rs` — Update function signatures

```rust
pub mod model;
pub mod capability;

pub use model::ModelCommands;
pub use capability::CapabilityCommands;
```

Each command method now takes `&AppContext` (read-only) or `&mut AppContext` (mutating).

### 8. `src/main.rs` — Update command dispatch

```rust
#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let mut ctx = AppContext::new()?;

    let result = match cli.command {
        Some(Commands::Model(subcmd)) => run_model_command(&mut ctx, subcmd).await,
        Some(Commands::Capability(subcmd)) => run_capability_command(&mut ctx, subcmd).await,
        // ...
    };
}
```

Functions that only read (model list, model info, capability list, capability info) receive `&ctx` or `&ctx.config`. Functions that mutate (model setup, capability setup) receive `&mut ctx`.

### 9. Tests

- `config/shell.rs` tests — no changes needed (uses `detect_shell()` directly)
- `config/exports.rs` tests — no changes needed (uses `Exporter` directly)
- Add new tests for `Config::new()` — verify directory creation, loading from disk, merging config.yaml with component files
- Add new tests for `Config::insert_model()` — verify HashMap population and file write
- Add new tests for `Config::remove_*()` — verify removal from HashMap and file delete
- Integration tests: `model list` displays bundled and configured models correctly

## Execution Order

1. Rewrite `Config` lifecycle in `config/mod.rs` (new, save, mutation methods)
2. Add `AppContext` in `main.rs`, wire command dispatch with context passing
3. Update `model list` (merged bundled + configured)
4. Update `model info` (show configuration state)
5. Update `model setup` (use mutation methods)
6. Update `capability list` and `capability setup`
7. Update provider commands (when added) with same pattern
8. Add tests for Config lifecycle
9. Run `cargo test` to verify everything compiles and passes
