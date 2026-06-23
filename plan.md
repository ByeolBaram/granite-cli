# granite-cli: Universal Model Adapter with Capabilities

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Build a Rust + ratatui CLI tool that seamlessly integrates Granite/Mellea AI models and capabilities into existing external tools (e.g., Claude CLI, Hermes CLI, IDEs) without modifying the tools' core configurations.

**Branding Context:**
- **Granite**: The user-facing value layer (the models and capabilities).
- **Mellea**: The generative computing engine layer used to maximize Granite value. Many Granite capabilities are out-of-the-box instantiation of Mellea programs.

**Scope:** The MVP focuses on model registry, provider matching, capability registry, tool overlay management, and a simple REPL chatbot (`run`), with a TUI and additional polish in later phases.

---

## 1. Architecture

### 1.1 Config Structure
All user configuration is stored in `~/.config/granite-cli/`:
```yaml
models.yaml          # Model registry (id, metadata, HF links, specs)
providers.yaml       # Provider registry (name, type, endpoint, auth, API capabilities)
routing.yaml         # Per-model provider priority lists
shell.yaml           # Shell detection + export destination
tools/
  └── <tool>.yaml    # Tool-specific overlay definitions (env vars, args to inject)
capabilities/
  ├── docling.yaml   # Document conversion capability config
  ├── vision.yaml    # Visual analysis capability config
  ├── speech.yaml    # Audio transcription/translation capability config
  └── compiler.yaml  # Mellea skills compiler capability config
```

**Configuration Philosophy:**
- **Single-instance semantics**: Each configured item (model, provider, capability) has one canonical configuration referenced by ID throughout the dependency graph
- **Content-hash caching**: The DI factory uses content hash as the cache key for resolved instances
- **Separate files**: Intentional separation allows independent configuration of items that may appear multiple times in the dependency graph

### 1.2 Core Patterns

#### Dependency Injection (DI) Factory
Instead of an explicit dependency graph resolver, granite-cli uses a **lazy Factory** pattern. Each command declares the abstract types it needs (e.g., a `ModelProvider`, a `Capability`). The Factory fulfills these dependencies lazily:
1. If the dependency is already configured, it returns the implementation (cached by content hash).
2. If not, it triggers the appropriate setup flow (depth-first resolution).
3. Capabilities can declare dependencies on models, providers, external tools, or other capabilities.
4. Cycle detection prevents circular capability dependencies.

**Auto-configuration with Override:**
- Factory auto-configures with intelligent defaults when possible
- Always presents auto-configured values to user for review/modification
- Critical for model variant selection (size, quantization, format)

*Example:* `run` needs a `ModelProvider` -> Factory checks if model is registered -> if not, triggers `model setup` -> `model setup` checks for provider -> triggers `provider setup` -> returns configured provider.

*Capability Example:* `capability setup vision` needs a vision model -> Factory checks if vision model is registered -> if not, triggers `model setup granite-vision` -> resolves provider -> returns configured capability.

*Capability Dependency Example:* `capability setup document-qa` depends on `docling` capability -> Factory resolves docling first -> then configures document-qa.

#### Model Selection and Variants
Models are organized in a hierarchy:
1. **Model Family**: Granite, Granite Vision, Granite Speech, Granite Guardian, etc.
2. **Version**: granite-3.3, granite-4.0, granite-4.1
3. **Size**: granite-4.1-3b, granite-4.1-8b
4. **Format**: safetensors, GGUF, ONNX
5. **Precision/Quantization**: safetensors/BF16, GGUF/Q8_0, GGUF/Q4_K_M

**ModelRecommender Component:**
A separate component sits between the Factory and registries to recommend appropriate model variants:
```rust
pub struct ModelRecommender {
    fn recommend_variant(
        &self,
        model_family: &str,
        provider: &dyn Provider,
        hardware_profile: &HardwareProfile,
        constraints: &ModelConstraints,
    ) -> Vec<ModelVariant>;
}
```

The Provider trait exposes:
```rust
fn supported_formats(&self) -> Vec<ModelFormat>;
fn supported_precisions(&self) -> Vec<Precision>;
fn can_run_model(&self, variant: &ModelVariant) -> bool;
```

**Compatibility Matrix:**
- Hosted providers typically don't offer format/precision qualification
- Local providers support various format/precision combinations
- User's hardware capacity (CPU, GPU, RAM, VRAM) affects local provider recommendations
- ModelRecommender implementation is deferred but architecture is in place

#### Provider Matching
Tools have their own supported API surfaces (e.g., Claude CLI supports Anthropic API and OpenAI-compatible). When a user runs `configure <tool>`, granite-cli:
1. Looks up the tool's supported API surfaces (hardcoded in tool adapters).
2. Scans the configured providers, checking their declared API capabilities.
3. Intersects the two sets and presents matching providers as options.
*Supported APIs:* OpenAI `/chat/completions`, Ollama `/api/chat`, Anthropic `/v1/messages`.

**Provider Failover:**
- Failover happens at launch time only, before tool starts
- System tests providers in priority order and selects one that works
- Once tool is launched, it communicates directly with the selected provider (no proxy)
- Ensures tool gets a working configuration (e.g., fall back to local if hosted rate-limited)

#### Capability Registry
Capabilities are first-class citizens alongside models and providers. Each capability:
- Has a unique identifier (e.g., `docling`, `vision`, `speech`, `compiler`)
- Declares its dependencies (models, providers, external tools, other capabilities)
- Implements execution hooks for different lifecycle phases
- Provides metadata (description, version, requirements)

**Capability Trait with Execution Hooks:**
```rust
pub trait Capability {
    // Metadata
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn dependencies(&self) -> Vec<Dependency>;
    
    // Execution hooks (all optional with NoOp defaults)
    async fn on_setup(&self, factory: &Factory) -> Result<()> { Ok(()) }
    async fn on_configure(&self, tool: &ToolConfig) -> Result<ConfigureResult> { Ok(ConfigureResult::default()) }
    async fn on_pre_launch(&self, context: &LaunchContext) -> Result<()> { Ok(()) }
    async fn on_post_launch(&self, context: &LaunchContext) -> Result<()> { Ok(()) }
    async fn on_shutdown(&self, context: &LaunchContext) -> Result<()> { Ok(()) }
    fn runtime_bindings(&self) -> Vec<EnvBinding> { vec![] }
}

pub enum Dependency {
    Model { id: String, required: bool },
    Provider { id: String, required: bool },
    ExternalTool { name: String, check_command: String },
    Capability { id: String, required: bool },
}
```

**Execution Hook Patterns:**
- **on_setup**: One-time initialization (e.g., download model weights, check external tools)
- **on_configure**: Runs during `configure <tool>` (e.g., compile skills, generate config files)
- **on_pre_launch**: Runs before tool launches (e.g., start background services)
- **on_post_launch**: Runs after tool starts (e.g., verify connections)
- **on_shutdown**: Cleanup when tool exits (e.g., stop background services)
- **runtime_bindings**: Returns environment variables for tool overlay

This pattern is more flexible than fixed capability "types" - capabilities implement only the hooks they need.

#### Tool Adapters
Tool adapters are **hard-coded** in granite-cli and translate granite capabilities into each tool's native configuration surface.

**Tool-Specific Configuration:**
- Every tool has its own config surface (env vars, config files, CLI flags)
- Granite-cli has no influence over tool config surfaces
- Per-tool adapters are essential and version-aware
- Adapters may need per-version logic as tools evolve

**Tool Adapter Structure:**
```rust
pub trait ToolAdapter {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn supported_apis(&self) -> Vec<ApiSurface>;
    
    // Version handling
    fn detect_version(&self) -> Result<Version>;
    fn supported_version_range(&self) -> VersionRange;
    
    // Version-aware configuration
    fn env_var_mapping(&self, version: &Version) -> HashMap<String, String>;
    fn config_file_path(&self, version: &Version) -> Option<PathBuf>;
    fn capability_binding(&self, capability: &Capability, version: &Version) -> BindingStrategy;
    
    // Lifecycle management (delegated to adapter)
    fn on_launch(&self, context: &LaunchContext) -> Result<()>;
    fn on_exit(&self, context: &LaunchContext) -> Result<()>;
}
```

**Version Management:**
- One adapter per tool that internally handles multiple versions
- Adapter implements version detection (tool-specific logic)
- Version-aware methods branch on detected version
- No persistent granite-cli daemon - lifecycle concerns delegated to providers and tools

**Skill Binding:**
- Adapters convert compiled Mellea skills into tool-specific formats
- Example: Hermes might want JSON, another tool might want Python modules
- Skills are invoked by the tool, not by granite-cli
- Granite-cli ensures skills are available in the right format

#### Tool Overlays
Granite-cli injects configuration into external tools via **non-invasive overlays**:
- **Overlay Mode (`launch`)**: Sets environment variables in the subprocess environment. Zero file modifications. Other terminal sessions calling the tool directly remain unaffected.
- **Export Mode (`configure --export`)**: User explicitly opts in to write environment variables to their detected shell profile (e.g., `~/.bash_profile`). Hardens the overlay into the tool's real config.

Capabilities contribute to overlays through their `runtime_bindings()` method and tool adapters translate these into tool-specific configuration.

### 1.3 Shell Introspection
Instead of hardcoding `~/.zshrc` or `~/.bashrc`, the CLI detects the user's shell via `$SHELL` and common startup file patterns.
```yaml
# shell.yaml (auto-detected)
shell: bash
export_file: ~/.bash_profile   # or ~/.bashrc if profile doesn't exist
export_format: export {VAR}="{VALUE}"
```
Users can override this via `granite-cli configure shell --file ~/.my_profile`.

---

## 2. Rust Project Structure

```
granite-cli/
├── Cargo.toml
├── src/
│   ├── main.rs              # Entry point + CLI routing
│   ├── config/
│   │   ├── mod.rs           # Config loading/saving (serde_yaml)
│   │   ├── shell.rs         # Shell detection (bash, zsh, etc.)
│   │   └── exports.rs       # Logic to modify shell profiles
│   ├── registry/
│   │   ├── mod.rs           # Registry trait + common logic
│   │   ├── models.rs        # Static model registry (bundled)
│   │   ├── providers.rs     # Static provider registry + capabilities
│   │   ├── capabilities.rs  # Static capability registry (bundled)
│   │   └── tools.rs         # Static tool adapters (env var maps)
│   ├── providers/
│   │   ├── mod.rs           # Provider trait definition
│   │   ├── openai_compat.rs # OpenAI /chat/completions client
│   │   ├── ollama.rs        # Ollama /chat client
│   │   └── anthropic.rs     # Anthropic /v1/messages client
│   ├── capabilities/
│   │   ├── mod.rs           # Capability trait definition
│   │   ├── docling.rs       # Document conversion capability
│   │   ├── vision.rs        # Visual analysis capability
│   │   ├── speech.rs        # Audio transcription/translation capability
│   │   └── compiler.rs      # Mellea skills compiler capability (abstraction)
│   ├── di/
│   │   ├── mod.rs           # Factory pattern
│   │   ├── resolver.rs      # Lazy dependency resolution + setup triggering
│   │   ├── graph.rs         # Dependency graph validation (cycle detection)
│   │   └── recommender.rs   # Model variant recommendation logic
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── model.rs         # model info / model setup
│   │   ├── provider.rs      # provider list / provider setup / provider health
│   │   ├── capability.rs    # capability info / capability setup / capability list
│   │   ├── run.rs           # REPL chatbot (with web-fetch)
│   │   ├── configure.rs     # Provider-to-Tool matching wizard + capability binding
│   │   └── launch.rs        # Subprocess wrapper + lifecycle management
│   └── utils/
│       ├── mod.rs
│       ├── web_fetch.rs     # Simple URL fetch + HTML-to-markdown
│       └── hardware.rs      # Hardware detection for model recommendations
```

---

## 3. Core Logic & Flows

### 3.1 The `configure` Flow
1. User runs `granite-cli configure claude`.
2. CLI looks up `claude`'s hardcoded supported API surfaces (e.g., Anthropic Messages, OpenAI Chat).
3. CLI scans `~/.config/granite-cli/providers.yaml` for providers that support those surfaces.
4. User selects a matching provider.
5. **Provider failover check**: Test selected provider, fall back to alternatives if needed.
6. CLI prompts user to select model (filtered by provider capabilities).
7. **ModelRecommender**: Suggest appropriate model variants based on provider and hardware.
8. Display available capabilities compatible with tool.
9. Prompt user to enable/disable each capability.
10. For each enabled capability:
    - Check if capability is configured (trigger `capability setup` if needed)
    - Resolve capability dependencies (including other capabilities)
    - Run capability's `on_configure` hook
11. Generate environment variable overlay (via tool adapter).
12. Tool adapter converts capability bindings to tool-specific format.
13. Save tool config to `tools/<tool-id>.yaml`.
14. Offer to export to shell profile.

### 3.2 The `launch` / `run` Flow
1. User runs `granite-cli run` (or `launch claude`).
2. The DI Factory is invoked to fulfill the required `ModelProvider`.
3. Factory checks if the model is in the local registry. If missing, triggers `model setup` depth-first.
4. Factory resolves the configured provider (e.g., OpenAI-compatible client).
5. **For `launch`**: 
   - CLI reads the tool's overlay config including capability bindings
   - Detect tool version via adapter
   - For each enabled capability, run `on_pre_launch` hook
   - Build environment variable overlay (via tool adapter's version-aware methods)
   - Spawn subprocess with overlay environment
   - Run capability `on_post_launch` hooks
   - Pass through tool arguments
   - Target tool (e.g., `claude`) is exec'd transparently
   - Monitor subprocess
   - On exit, run capability `on_shutdown` hooks (adapter-managed lifecycle)
6. **For `run`**: The REPL chatbot uses the provider client to stream responses, with in-memory context history.

### 3.3 The `run` Chatbot
- **Interface**: Simple REPL style (Phase 1-3) using `dialoguer` or `ratatui` (Phase 4).
- **Context**: Maintains conversation history in memory (ephemeral, per-session).
- **Web Fetch**: Users can paste a URL into the chat. `utils::web_fetch` grabs the content, strips HTML, converts to markdown, and injects it into the context. (No web search in MVP).
- **Inference**: Handles its own inference via the resolved provider client (OpenAI/Ollama/Anthropic).
- **Capabilities**: Integration with capabilities is deferred and may be removed entirely.

### 3.4 Capability Setup Flow
1. User runs `granite-cli capability setup <capability-id>` (or triggered by `configure`).
2. CLI loads the capability definition from the static registry.
3. Factory resolves capability dependencies (depth-first):
   - Models: Triggers `model setup` if needed
   - Providers: Triggers `provider setup` if needed
   - External tools: Checks availability, prompts for installation if missing
   - Other capabilities: Recursively resolves with cycle detection
4. Run capability's `on_setup` hook.
5. CLI prompts for capability-specific configuration.
6. Configuration is saved to `~/.config/granite-cli/capabilities/<capability-id>.yaml`.

### 3.5 Dependency Resolution with Cycle Detection
```rust
// di/graph.rs
pub struct DependencyGraph {
    fn build_graph(&self, root: &str) -> Result<Graph>;
    fn detect_cycles(&self, graph: &Graph) -> Result<Vec<Cycle>>;
    fn topological_sort(&self, graph: &Graph) -> Result<Vec<String>>;
}
```

When resolving capabilities:
1. Build dependency graph from capability definitions
2. Detect cycles using DFS
3. If cycles found, provide clear error message
4. Otherwise, resolve in topological order

---

## 4. Technical Stack
- **Language:** Rust 2024 Edition
- **Config Parsing:** `serde` + `serde_yaml`
- **HTTP Client:** `reqwest` (streaming support for provider clients)
- **CLI Wizard (MVP):** `dialoguer`
- **TUI (Phase 4):** `crossterm` + `ratatui`
- **Async Runtime:** `tokio` for async operations
- **Distribution:** Single static binary. Core logic is self-contained; `~/.config/granite-cli/` is only for user state.

---

## 5. Implementation Phases

### Phase 1: Foundation (Config + Model Registry + Capability Foundation)

**Goal:** Establish the core configuration system, model registry, and capability abstraction layer.

#### 1.1 Project Scaffolding
- Initialize Rust project with `cargo init`
- Set up `Cargo.toml` with dependencies:
  - `clap` (CLI argument parsing with derive macros)
  - `serde` + `serde_yaml` (config serialization)
  - `anyhow` (error handling)
  - `dialoguer` (interactive prompts)
  - `dirs` (cross-platform config directory detection)
  - `tokio` (async runtime)
- Create module structure matching project layout
- Set up basic CLI routing in `main.rs` with subcommands: `model`, `capability`, `provider`, `configure`, `launch`, `run`

#### 1.2 Config System
- **`config/mod.rs`**: Define core config structs
  ```rust
  pub struct Config {
      pub models: HashMap<String, ModelConfig>,
      pub providers: HashMap<String, ProviderConfig>,
      pub capabilities: HashMap<String, CapabilityConfig>,
      pub routing: RoutingConfig,
      pub shell: ShellConfig,
      pub tools: HashMap<String, ToolConfig>,
  }
  ```
- Implement config loading/saving with YAML
- Support config directory creation on first run
- Handle missing config files gracefully
- **`config/shell.rs`**: Shell detection logic
  - Detect shell from `$SHELL` environment variable
  - Identify startup files (`.bashrc`, `.bash_profile`, `.zshrc`, `.zprofile`, etc.)
  - Provide shell-specific export format templates
  - Support common shells: bash, zsh, fish
- **`config/exports.rs`**: Shell profile modification (skeleton)
  - Define interface for reading/writing shell profiles
  - Add granite-cli comment markers for identification
  - Defer full implementation to Phase 3

#### 1.3 Model Registry
- **`registry/mod.rs`**: Define `Registry` trait
  ```rust
  pub trait Registry<T> {
      fn list(&self) -> Vec<&T>;
      fn get(&self, id: &str) -> Option<&T>;
      fn search(&self, query: &str) -> Vec<&T>;
  }
  ```
- **`registry/models.rs`**: Static model registry
  - Define `ModelDefinition` struct with metadata:
    - `id`: Unique identifier (e.g., `granite-3.1-8b-instruct`)
    - `family`: Model family (Granite, Granite Vision, etc.)
    - `version`: Version string
    - `size`: Model size in parameters
    - `context_length`: Maximum context window
    - `model_type`: Enum (Text, Vision, Speech, Embedding)
    - `huggingface_repo`: HF repository path
    - `required_provider_capabilities`: List of API surfaces
    - `variants`: Available format/precision combinations
  - Bundle initial model definitions:
    - Granite 3.1 family (3b, 8b, 20b)
    - Granite Vision
    - Granite Speech
    - Granite Guardian
  - Implement `Registry<ModelDefinition>` trait

#### 1.4 Capability Abstraction
- **`capabilities/mod.rs`**: Define `Capability` trait
  ```rust
  #[async_trait]
  pub trait Capability: Send + Sync {
      // Metadata
      fn id(&self) -> &str;
      fn name(&self) -> &str;
      fn description(&self) -> &str;
      fn dependencies(&self) -> Vec<Dependency>;
      
      // Execution hooks (all optional with NoOp defaults)
      async fn on_setup(&self, factory: &Factory) -> Result<()> { Ok(()) }
      async fn on_configure(&self, tool: &ToolConfig) -> Result<ConfigureResult> { 
          Ok(ConfigureResult::default()) 
      }
      async fn on_pre_launch(&self, context: &LaunchContext) -> Result<()> { Ok(()) }
      async fn on_post_launch(&self, context: &LaunchContext) -> Result<()> { Ok(()) }
      async fn on_shutdown(&self, context: &LaunchContext) -> Result<()> { Ok(()) }
      fn runtime_bindings(&self) -> Vec<EnvBinding> { vec![] }
  }
  
  pub enum Dependency {
      Model { id: String, required: bool },
      Provider { id: String, required: bool },
      ExternalTool { name: String, check_command: String },
      Capability { id: String, required: bool },
  }
  
  pub struct ConfigureResult {
      pub success: bool,
      pub artifacts: Vec<PathBuf>,
      pub messages: Vec<String>,
  }
  
  pub struct LaunchContext {
      pub tool_id: String,
      pub tool_version: Version,
      pub working_dir: PathBuf,
      pub env_vars: HashMap<String, String>,
  }
  
  pub struct EnvBinding {
      pub key: String,
      pub value: String,
  }
  ```
- **`registry/capabilities.rs`**: Static capability registry
  - Define `CapabilityDefinition` struct with metadata
  - Bundle initial capability definitions (docling, vision, speech, compiler)
  - Implement `Registry<CapabilityDefinition>` trait

#### 1.5 Commands: Model Management
- **`commands/model.rs`**: Implement model commands
  - `model list`: Display all available models in a table
    - Show: ID, family, size, type, context length
    - Filter by type (--type text|vision|speech)
  - `model info <model-id>`: Show detailed model information
    - Display all metadata
    - Show available variants
    - List required provider capabilities
  - `model setup <model-id>`: Interactive wizard to configure a model (skeleton)
    - Check if model exists in registry
    - Display model information
    - Defer provider selection to Phase 2
    - Save placeholder configuration to `models.yaml`

#### 1.6 Commands: Capability Management (Skeleton)
- **`commands/capability.rs`**: Implement basic capability commands
  - `capability list`: Display all available capabilities
    - Show: ID, name, description, dependencies
  - `capability info <capability-id>`: Show detailed capability information
    - Display metadata
    - List dependencies (models, providers, tools, other capabilities)
    - Show execution hooks implemented
  - `capability setup <capability-id>`: Placeholder (full implementation in Phase 2)
    - Display "Not yet implemented" message
    - Show what dependencies would be needed

#### 1.7 Testing & Validation
- Unit tests for config serialization/deserialization
- Unit tests for shell detection logic
- Unit tests for registry implementations
- Integration test: `model list` displays bundled models
- Integration test: `model info` shows correct metadata
- Integration test: `capability list` displays bundled capabilities
- Integration test: Config directory is created on first run
- Manual test: `model setup` wizard displays correctly

**Phase 1 Deliverables:**
- Working CLI with `model` and `capability` subcommands
- Config system with YAML persistence
- Static registries for models and capabilities
- Shell detection utility
- Foundation for DI system (interfaces defined)
- Capability trait with execution hooks pattern

---

### Phase 2: Provider Clients + DI System + Capability Infrastructure

**Goal:** Implement provider clients, complete the DI factory with dependency resolution, and build capability infrastructure.

#### 2.1 Provider Abstraction
- **`providers/mod.rs`**: Define `Provider` trait
  ```rust
  #[async_trait]
  pub trait Provider: Send + Sync {
      fn id(&self) -> &str;
      fn api_capabilities(&self) -> Vec<ApiSurface>;
      
      // Model support
      fn supported_formats(&self) -> Vec<ModelFormat>;
      fn supported_precisions(&self) -> Vec<Precision>;
      fn can_run_model(&self, variant: &ModelVariant) -> bool;
      
      // Inference
      async fn chat_completion(&self, request: ChatRequest) -> Result<ChatResponse>;
      async fn stream_chat(&self, request: ChatRequest) -> Result<impl Stream<Item = ChatChunk>>;
      
      // Health
      async fn health_check(&self) -> Result<HealthStatus>;
  }
  
  pub enum ApiSurface {
      OpenAIChat,         // /v1/chat/completions
      OllamaChat,         // /api/chat
      AnthropicMessages,  // /v1/messages
  }
  
  pub enum ModelFormat {
      Safetensors,
      GGUF,
      ONNX,
  }
  
  pub enum Precision {
      BF16,
      FP16,
      FP8,
      Q8_0,
      Q4_K_M,
      // ... other quantizations
  }
  
  pub struct HealthStatus {
      pub healthy: bool,
      pub latency: Duration,
      pub error: Option<String>,
  }
  ```
- **`registry/providers.rs`**: Static provider registry
  - Define `ProviderDefinition` struct with metadata
  - Bundle common provider templates:
    - OpenAI (hosted)
    - Anthropic (hosted)
    - Ollama (local)
    - IBM watsonx.ai (hosted)
  - Implement `Registry<ProviderDefinition>` trait

#### 2.2 Provider Implementations
- **`providers/openai_compat.rs`**: OpenAI-compatible client
  - Implement `Provider` trait
  - Use `reqwest` for HTTP requests
  - Support streaming via Server-Sent Events (SSE)
  - Handle authentication (API key in header)
  - Map request/response to OpenAI format
  - Implement health check (test `/v1/models` endpoint)
- **`providers/ollama.rs`**: Ollama client
  - Implement `Provider` trait
  - Use Ollama's `/api/chat` endpoint
  - Support streaming
  - Handle local endpoint (default: `http://localhost:11434`)
  - Implement health check (test `/api/tags` endpoint)
  - Report supported formats/precisions based on local models
- **`providers/anthropic.rs`**: Anthropic client
  - Implement `Provider` trait
  - Use Anthropic's `/v1/messages` endpoint
  - Support streaming
  - Handle authentication (API key + version header)
  - Implement health check (minimal request)

#### 2.3 DI Factory System
- **`di/mod.rs`**: Implement `Factory` struct
  ```rust
  pub struct Factory {
      config: Arc<RwLock<Config>>,
      providers: Arc<RwLock<HashMap<String, Arc<dyn Provider>>>>,
      capabilities: Arc<RwLock<HashMap<String, Arc<dyn Capability>>>>,
      cache: Arc<RwLock<HashMap<String, CachedInstance>>>,
  }
  
  impl Factory {
      pub async fn resolve_model(&self, id: &str) -> Result<Arc<dyn Provider>>;
      pub async fn resolve_capability(&self, id: &str) -> Result<Arc<dyn Capability>>;
      pub async fn resolve_provider(&self, id: &str) -> Result<Arc<dyn Provider>>;
      
      // Auto-configure with user override
      async fn auto_configure<T>(&self, item: &T, options: Vec<ConfigOption>) -> Result<T>;
  }
  
  struct CachedInstance {
      content_hash: String,
      instance: Arc<dyn Any + Send + Sync>,
  }
  ```
- **`di/resolver.rs`**: Lazy resolution logic
  - Implement depth-first dependency resolution
  - Trigger setup flows when dependencies are missing
  - Cache resolved instances by content hash
  - Auto-configure with intelligent defaults
  - Present auto-configured values to user for review
  - Handle user overrides
- **`di/graph.rs`**: Dependency graph validation
  - Build dependency graph from capability definitions
  - Detect cycles using DFS
  - Provide clear error messages for invalid configurations
  - Support topological sorting for resolution order
- **`di/recommender.rs`**: Model variant recommendation
  - Implement `ModelRecommender` struct
  - Consider provider capabilities (formats, precisions)
  - Consider hardware profile (CPU, GPU, RAM, VRAM)
  - Rank variants by suitability
  - Provide recommendations with explanations

#### 2.4 Hardware Detection
- **`utils/hardware.rs`**: Hardware profiling
  - Detect CPU (cores, architecture)
  - Detect GPU (vendor, VRAM)
  - Detect system RAM
  - Create `HardwareProfile` struct
  - Use for model recommendations

#### 2.5 Capability Implementations (Stubs)
- **`capabilities/docling.rs`**: Document conversion capability (stub)
  - Implement `Capability` trait
  - Define dependencies: External tool (`docling`)
  - Implement `on_setup`: Check for `docling` installation
  - Implement `runtime_bindings`: Return skill path environment variables
  - Note: Docling invoked by tool, not granite-cli
- **`capabilities/vision.rs`**: Visual analysis capability (stub)
  - Implement `Capability` trait
  - Define dependencies: `granite-vision` model
  - Implement `on_setup`: Resolve vision model
  - Implement `on_pre_launch`: Start vision model runtime (placeholder)
  - Implement `on_shutdown`: Stop vision runtime
  - Implement `runtime_bindings`: Return endpoint environment variables
- **`capabilities/speech.rs`**: Audio transcription capability (stub)
  - Implement `Capability` trait
  - Define dependencies: `granite-speech` model
  - Implement `on_setup`: Resolve speech model
  - Implement `runtime_bindings`: Return model path environment variables
- **`capabilities/compiler.rs`**: Mellea skills compiler capability (stub)
  - Implement `Capability` trait
  - Define dependencies: `granite-guardian` model, Mellea compiler tool
  - Implement `on_setup`: Resolve guardian model, check for compiler
  - Implement `on_configure`: Placeholder for compilation logic
  - Note: Full implementation deferred - this is an abstraction placeholder

#### 2.6 Web Fetch Utility
- **`utils/web_fetch.rs`**: URL content fetching
  - Use `reqwest` to fetch URL content
  - Parse HTML and convert to markdown (use `html2md` crate)
  - Handle common errors (404, timeout, etc.)
  - Support basic authentication if needed
  - Limit content size to prevent memory issues
  - Add user-agent header

#### 2.7 Commands: Run Chatbot
- **`commands/run.rs`**: Implement REPL chatbot
  - Accept optional model ID argument
  - Use Factory to resolve model provider
  - Implement conversation loop:
    - Read user input (use `dialoguer::Input`)
    - Detect URLs in input, fetch content with `web_fetch`
    - Build chat request with conversation history
    - Stream response from provider
    - Display response incrementally
    - Maintain in-memory conversation history
  - Support special commands:
    - `/clear`: Clear conversation history
    - `/exit` or `/quit`: Exit chatbot
    - `/model <id>`: Switch to different model
    - `/help`: Show available commands
  - Handle errors gracefully (network issues, API errors)
  - Note: Capability integration deferred, may be removed

#### 2.8 Commands: Provider Management
- **`commands/provider.rs`**: Implement provider commands
  - `provider list`: Display configured providers
    - Show: ID, type, endpoint, API capabilities
    - Indicate health status (cached)
  - `provider setup <provider-id>`: Interactive wizard to configure a provider
    - Load provider template from registry
    - Prompt for endpoint URL
    - Prompt for authentication (API key, etc.)
    - Test connection with health check
    - Save configuration to `providers.yaml`
  - `provider health [<provider-id>]`: Check provider health
    - Test specified provider or all providers
    - Display latency and status
    - Cache results with TTL

#### 2.9 Commands: Capability Setup (Full Implementation)
- **`commands/capability.rs`**: Complete capability setup
  - `capability setup <capability-id>`: Full implementation
    - Load capability definition from registry
    - Build dependency graph
    - Check for cycles
    - Use Factory to resolve dependencies in topological order
    - For each dependency:
      - If model: Trigger `model setup` with ModelRecommender
      - If provider: Trigger `provider setup`
      - If external tool: Check availability, provide install instructions
      - If capability: Recursively resolve
    - Run capability's `on_setup` hook
    - Prompt for capability-specific configuration
    - Save configuration to `capabilities/<capability-id>.yaml`
  - `capability test <capability-id>`: Test capability (if applicable)
    - Run basic functionality test
    - Report success/failure

#### 2.10 Commands: Model Setup (Complete)
- **`commands/model.rs`**: Complete model setup implementation
  - `model setup <model-id>`: Full interactive wizard
    - Load model definition from registry
    - Display model information and variants
    - Prompt for provider selection
    - Use ModelRecommender to suggest variants
    - Present recommendations with explanations
    - Allow user to override recommendations
    - Test provider with health check
    - Implement failover: try alternatives if selected provider fails
    - Save configuration to `models.yaml`

#### 2.11 Testing & Validation
- Unit tests for each provider implementation
- Unit tests for Factory resolution logic
- Unit tests for dependency graph cycle detection
- Unit tests for ModelRecommender
- Unit tests for web_fetch utility
- Integration test: `run` chatbot connects to provider and streams response
- Integration test: `capability setup` resolves dependencies correctly
- Integration test: Cycle detection prevents circular dependencies
- Integration test: Web fetch converts HTML to markdown
- Integration test: Provider health checks work correctly
- Integration test: ModelRecommender suggests appropriate variants
- Manual test: Chat with different providers (OpenAI, Ollama, Anthropic)
- Manual test: Paste URL in chat, verify content is fetched and injected
- Manual test: Setup capability with complex dependency chain

**Phase 2 Deliverables:**
- Working provider clients (OpenAI, Ollama, Anthropic)
- Complete DI Factory with lazy resolution and auto-configuration
- Dependency graph validation with cycle detection
- ModelRecommender for variant selection
- Functional REPL chatbot with web fetch
- Capability setup infrastructure with dependency resolution
- Provider management commands with health checks
- Hardware detection for model recommendations

---

### Phase 3: Tool Wrapping + Capability Integration (with Stubs)

**Goal:** Implement tool overlay system, integrate capability stubs into tool configurations, and enable launching tools with Granite enhancements.

#### 3.1 Tool Adapter System
- **`registry/tools.rs`**: Static tool adapter registry
  - Define `ToolAdapter` trait:
    ```rust
    #[async_trait]
    pub trait ToolAdapter: Send + Sync {
        fn id(&self) -> &str;
        fn name(&self) -> &str;
        fn description(&self) -> &str;
        fn supported_apis(&self) -> Vec<ApiSurface>;
        
        // Version handling
        fn detect_version(&self) -> Result<Version>;
        fn supported_version_range(&self) -> VersionRange;
        
        // Version-aware configuration
        fn env_var_mapping(&self, version: &Version) -> HashMap<String, String>;
        fn config_file_path(&self, version: &Version) -> Option<PathBuf>;
        
        // Capability integration
        fn capability_binding(
            &self,
            capability: &dyn Capability,
            version: &Version,
        ) -> Result<BindingStrategy>;
        
        // Skill format conversion
        fn convert_skills(
            &self,
            skills_dir: &Path,
            version: &Version,
        ) -> Result<PathBuf>;
        
        // Lifecycle management (delegated to adapter)
        async fn on_launch(&self, context: &LaunchContext) -> Result<()> { Ok(()) }
        async fn on_exit(&self, context: &LaunchContext) -> Result<()> { Ok(()) }
    }
    
    pub enum BindingStrategy {
        EnvironmentVariables(HashMap<String, String>),
        ConfigFile { path: PathBuf, content: String },
        Both {
            env_vars: HashMap<String, String>,
            config_file: (PathBuf, String),
        },
    }
    ```
  - Implement adapters for common tools:
    - **Claude CLI**: Supports Anthropic Messages, OpenAI Chat
      - Version detection: `claude --version`
      - Env vars: `ANTHROPIC_API_KEY`, `ANTHROPIC_BASE_URL`
      - Skill format: JSON skill definitions
    - **Hermes CLI**: Supports OpenAI Chat, custom APIs
      - Version detection: `hermes --version`
      - Env vars: `OPENAI_API_KEY`, `OPENAI_BASE_URL`
      - Skill format: Python modules
    - **Generic OpenAI-compatible**: For custom tools
      - Env vars: `OPENAI_API_KEY`, `OPENAI_BASE_URL`
      - Skill format: Directory of JSON files
  - Implement `Registry<ToolAdapter>` trait

#### 3.2 Tool Configuration System
- **`config/mod.rs`**: Extend with `ToolConfig` struct
  ```rust
  pub struct ToolConfig {
      pub tool_id: String,
      pub tool_version: Version,
      pub provider_id: String,
      pub model_id: String,
      pub env_vars: HashMap<String, String>,
      pub capabilities: Vec<ConfiguredCapability>,
      pub export_to_shell: bool,
  }
  
  pub struct ConfiguredCapability {
      pub capability_id: String,
      pub enabled: bool,
      pub config: HashMap<String, String>,
  }
  ```
- Implement save/load for tool configs in `tools/` directory
- Support config versioning for tool updates

#### 3.3 Provider-Tool Matching Logic
- **`commands/configure.rs`**: Implement tool configuration wizard
  - `configure <tool-id>`: Interactive configuration flow
    1. Load tool adapter from registry
    2. Detect tool version
    3. Display tool information and supported APIs
    4. Scan configured providers for API compatibility
    5. Present matching providers to user (with recommendations)
    6. Prompt user to select provider
    7. **Provider failover**: Test selected provider, try alternatives if fails
    8. Prompt user to select model (filtered by provider capabilities)
    9. Use ModelRecommender for variant suggestions
    10. Display available capabilities compatible with tool
    11. Prompt user to enable/disable each capability
    12. For each enabled capability:
        - Check if capability is configured (trigger `capability setup` if needed)
        - Resolve capability dependencies (including other capabilities)
        - Run capability's `on_configure` hook
        - Get capability bindings via tool adapter
    13. Generate environment variable overlay
    14. Convert skills to tool-specific format (if applicable)
    15. Save tool config to `tools/<tool-id>.yaml`
    16. Offer to export to shell profile
  - `configure <tool-id> --export`: Export configuration to shell
    - Read tool config
    - Generate shell-specific export statements
    - Append to detected shell profile
    - Add granite-cli comment markers
    - Backup original file before modification
  - `configure <tool-id> --reset`: Remove tool configuration
    - Delete tool config file
    - Optionally remove from shell profile

#### 3.4 Launch System
- **`commands/launch.rs`**: Implement tool launching
  - `launch <tool-id> [-- <tool-args>]`: Launch tool with overlay
    1. Load tool config from `tools/<tool-id>.yaml`
    2. Load tool adapter from registry
    3. Detect tool version
    4. Resolve provider and model via Factory
    5. Build environment variable overlay:
       - Provider endpoint and authentication
       - Model identifier
       - Tool adapter's version-specific env vars
    6. For each enabled capability:
       - Run `on_pre_launch` hook
       - Get runtime bindings
       - Add to overlay
    7. Spawn subprocess with overlay environment
    8. Run tool adapter's `on_launch` hook
    9. Pass through tool arguments
    10. For each enabled capability, run `on_post_launch` hook
    11. Monitor subprocess
    12. On exit:
        - Run capability `on_shutdown` hooks
        - Run tool adapter's `on_exit` hook
  - Handle errors gracefully:
    - Tool not found
    - Configuration missing
    - Capability hook failures
    - Subprocess errors
  - Support `--dry-run` flag to show overlay without launching

#### 3.5 Export System
- **`config/exports.rs`**: Complete shell export implementation
  - Read existing shell profile
  - Find granite-cli section (between comment markers)
  - Update environment variables
  - Preserve user's existing configuration
  - Add comments for clarity
  - Backup original file before modification
  - Support removal of granite-cli section

#### 3.6 Testing & Validation
- Unit tests for provider-tool matching logic
- Unit tests for environment variable overlay generation
- Unit tests for tool adapter version detection
- Unit tests for skill format conversion
- Integration test: `configure` wizard completes successfully
- Integration test: `launch` starts tool with correct environment
- Integration test: Capability hooks execute in correct order
- Integration test: Tool adapter lifecycle methods called correctly
- Integration test: Skills converted to tool-specific format
- Manual test: Configure Claude CLI with Granite model
- Manual test: Launch Claude CLI, verify it uses Granite provider
- Manual test: Enable vision capability stub, verify bindings present
- Manual test: Export configuration to shell, verify persistence
- Manual test: `--dry-run` shows correct overlay

**Phase 3 Deliverables:**
- Working tool configuration wizard
- Tool launching with environment overlays
- Tool adapters with version awareness
- Capability integration (stubs with hooks)
- Skill format conversion per tool
- Shell export functionality
- Lifecycle management delegated to adapters
- Provider failover at launch time

---

### Phase 4: Polish & TUI

**Goal:** Enhance user experience with a TUI, improve error handling, add advanced features, and polish the overall system.

#### 4.1 TUI Framework Setup
- Add dependencies to `Cargo.toml`:
  - `ratatui` (TUI framework)
  - `crossterm` (terminal manipulation)
  - `syntect` (syntax highlighting)
  - `pulldown-cmark` (markdown parsing)
- Create TUI module structure:
  ```
  src/tui/
  ├── mod.rs           # TUI framework setup
  ├── app.rs           # Application state
  ├── ui.rs            # UI rendering
  ├── events.rs        # Event handling
  └── widgets/
      ├── chat.rs      # Chat widget
      ├── markdown.rs  # Markdown rendering widget
      └── wizard.rs    # Interactive wizard widget
  ```

#### 4.2 TUI Chat Interface
- **`tui/widgets/chat.rs`**: Implement chat widget
  - Display conversation history with markdown rendering
  - Syntax highlighting for code blocks
  - Scrollable message list
  - Input area with multi-line support
  - Status bar showing model, provider, token count
  - Support for special commands (same as REPL)
  - Visual indicators for streaming responses
- **`tui/widgets/markdown.rs`**: Markdown rendering
  - Parse markdown with `pulldown-cmark`
  - Render formatted text (bold, italic, headers)
  - Syntax highlight code blocks with `syntect`
  - Support tables and lists
- **`commands/run.rs`**: Add TUI mode
  - `run --tui` or `run -t`: Launch TUI chat interface
  - Maintain same functionality as REPL
  - Enhanced visual experience with markdown rendering
  - Keyboard shortcuts for navigation

#### 4.3 TUI Wizards
- **`tui/widgets/wizard.rs`**: Implement wizard widget
  - Step-by-step configuration flow
  - Visual progress indicator
  - Input validation with inline feedback
  - Navigation (next, back, cancel)
  - Support for different input types (text, select, multi-select)
- **`commands/model.rs`**: Add TUI wizard for model setup
  - `model setup --tui`: Launch TUI wizard
  - Visual model selection with descriptions
  - Provider selection with compatibility indicators
  - Variant recommendations with explanations
- **`commands/configure.rs`**: Add TUI wizard for tool configuration
  - `configure <tool> --tui`: Launch TUI wizard
  - Visual capability selection with descriptions
  - Real-time validation feedback
  - Progress tracking through configuration steps

#### 4.4 Priority Routing
- **`config/mod.rs`**: Exten
d `RoutingConfig`
  ```rust
  pub struct RoutingConfig {
      pub model_routes: HashMap<String, Vec<ProviderRoute>>,
  }
  
  pub struct ProviderRoute {
      pub provider_id: String,
      pub priority: u8,
      pub health_check: bool,
  }
  ```
- **`di/resolver.rs`**: Implement priority-based provider selection
  - Try providers in priority order
  - Skip unhealthy providers
  - Fallback to next provider on failure
  - Cache health check results (with TTL)
  - Failover happens at launch time only

#### 4.5 Enhanced Error Handling
- Create custom error types for different failure modes:
  - `ConfigError`: Configuration issues
  - `ProviderError`: Provider communication failures
  - `CapabilityError`: Capability setup/execution failures
  - `DependencyError`: Missing dependencies
  - `ToolAdapterError`: Tool adapter failures
- Provide actionable error messages with suggestions
- Add `--verbose` flag for detailed error output
- Implement error recovery strategies where possible
- Add context to errors (what was being attempted)

#### 4.6 Logging and Debugging
- Add `tracing` crate for structured logging
- Implement log levels (error, warn, info, debug, trace)
- Log to file: `~/.config/granite-cli/logs/granite-cli.log`
- Add `--log-level` flag to control verbosity
- Add `debug` command to dump configuration and state
- Log key events:
  - Dependency resolution steps
  - Provider health checks
  - Capability hook executions
  - Tool launches

#### 4.7 Documentation
- Create comprehensive README.md:
  - Installation instructions
  - Quick start guide
  - Configuration examples
  - Capability descriptions
  - Troubleshooting guide
- Create ARCHITECTURE.md:
  - System design overview
  - Component descriptions
  - Data flow diagrams
  - Extension points
- Create CONTRIBUTING.md:
  - Development setup
  - Code style guidelines
  - Testing requirements
  - Pull request process

#### 4.8 Testing & Validation
- Unit tests for TUI components
- Unit tests for priority routing logic
- Unit tests for error handling
- Integration test: TUI chat interface works correctly
- Integration test: Priority routing selects correct provider
- Integration test: Failover works at launch time
- Manual test: TUI wizards provide good user experience
- Manual test: Error messages are clear and actionable
- Manual test: Logging captures relevant information

**Phase 4 Deliverables:**
- TUI chat interface with markdown rendering
- TUI wizards for setup flows
- Priority routing with failover at launch time
- Enhanced error handling and logging
- Comprehensive documentation

---

### Phase 5: Full Capability Implementations

**Goal:** Fully implement the four core capabilities with production-ready quality.

#### 5.1 Document Conversion Capability (Docling)

**Overview:** Integrate IBM's Docling library to convert various document formats into markdown.

##### 5.1.1 Docling Integration
- **`capabilities/docling.rs`**: Full implementation
  - Check for Python and `docling` package installation
  - Provide installation instructions if missing (`pip install docling`)
  - Implement document conversion wrapper
  - Support multiple output formats (markdown, JSON, text)
  - Handle conversion errors gracefully
  - Note: Docling is invoked by the tool via skills, not directly by granite-cli

##### 5.1.2 Runtime Binding
- Environment variables for tools:
  - `GRANITE_DOCLING_ENABLED=true`
  - `GRANITE_DOCLING_SKILL_PATH=/path/to/docling/skill`
- Create skill definition that tools can use:
  - Skill name: `convert_document`
  - Input: File path
  - Output: Converted markdown content
  - Implementation: Calls `docling` via subprocess

##### 5.1.3 Configuration
- **`capabilities/docling.yaml`**:
  ```yaml
  enabled: true
  dependencies:
    external_tool:
      name: docling
      check_command: python -c "import docling"
  output_format: markdown
  supported_formats:
    - pdf
    - docx
    - pptx
    - xlsx
    - html
  max_file_size_mb: 50
  ```

##### 5.1.4 Testing
- Unit tests for skill definition generation
- Integration test: Verify docling installation check
- Integration test: Skill definition is correctly formatted
- Manual test: Tool can invoke docling skill successfully

#### 5.2 Visual Analysis Capability (Granite Vision)

**Overview:** Integrate Granite Vision model to analyze images.

##### 5.2.1 Vision Model Integration
- **`capabilities/vision.rs`**: Full implementation
  - Resolve `granite-vision` model via Factory
  - Implement vision runtime adapter (if needed)
  - Support multiple image formats (PNG, JPEG, GIF, WebP)
  - Handle image preprocessing if required

##### 5.2.2 Hybrid Runtime (if needed)
- If vision model requires special runtime:
  - Start vision model runtime adapter as background service
  - Health check endpoint
  - API endpoint for image analysis
- Environment variables for tools:
  - `GRANITE_VISION_ENABLED=true`
  - `GRANITE_VISION_ENDPOINT=http://localhost:<port>` (if hybrid)
  - `GRANITE_VISION_MODEL=granite-vision-3.1-8b`
- Create skill definition:
  - Skill name: `analyze_image`
  - Input: Image path or URL, optional prompt
  - Output: Analysis result

##### 5.2.3 Configuration
- **`capabilities/vision.yaml`**:
  ```yaml
  enabled: true
  dependencies:
    model:
      id: granite-vision-3.1-8b
      required: true
  # If hybrid runtime needed:
  background_service:
    command: granite-vision-server
    port: 8765
    health_check: http://localhost:8765/health
  supported_formats:
    - png
    - jpg
    - jpeg
    - gif
    - webp
  max_image_size_mb: 10
  ```

##### 5.2.4 Testing
- Unit tests for skill definition generation
- Integration test: Vision model resolution
- Integration test: Background service startup (if applicable)
- Manual test: Tool can invoke vision skill successfully

#### 5.3 Audio Transcription/Translation Capability (Granite Speech)

**Overview:** Integrate Granite Speech model to transcribe and translate audio.

##### 5.3.1 Speech Model Integration
- **`capabilities/speech.rs`**: Full implementation
  - Resolve `granite-speech` model via Factory
  - Support multiple audio formats (WAV, MP3, FLAC, OGG)
  - Handle audio preprocessing if required

##### 5.3.2 Runtime Binding
- Environment variables for tools:
  - `GRANITE_SPEECH_ENABLED=true`
  - `GRANITE_SPEECH_MODEL_PATH=/path/to/model`
  - `GRANITE_SPEECH_LANGUAGES=en,es,fr,de,zh`
- Create skill definitions:
  - Skill name: `transcribe_audio`
    - Input: Audio file path, optional language hint
    - Output: Transcription with segments
  - Skill name: `translate_audio`
    - Input: Audio file path, target language
    - Output: Translated text

##### 5.3.3 Configuration
- **`capabilities/speech.yaml`**:
  ```yaml
  enabled: true
  dependencies:
    model:
      id: granite-speech-1.0
      required: true
  supported_formats:
    - wav
    - mp3
    - flac
    - ogg
  supported_languages:
    - en
    - es
    - fr
    - de
    - zh
  max_audio_duration_minutes: 60
  ```

##### 5.3.4 Testing
- Unit tests for skill definition generation
- Integration test: Speech model resolution
- Manual test: Tool can invoke speech skills successfully

#### 5.4 Mellea Skills Compiler Capability (Abstraction)

**Overview:** Placeholder for Mellea Skills Compiler integration. Full implementation deferred.

##### 5.4.1 Compiler Abstraction
- **`capabilities/compiler.rs`**: Abstraction implementation
  - Check for Mellea compiler installation
  - Resolve `granite-guardian` model via Factory
  - Implement `on_configure` hook with placeholder logic
  - Define expected input/output structure
  - Document what the full implementation should do

##### 5.4.2 ConfigTime Execution Pattern
- Run during `configure <tool>` when compiler capability is enabled
- Input: Raw skills from `~/.config/granite-cli/skills/raw/`
- Output: Compiled skills to `~/.config/granite-cli/skills/compiled/`
- Environment variables for tools:
  - `GRANITE_SKILLS_PATH=~/.config/granite-cli/skills/compiled`
  - `GRANITE_SKILLS_COMPILER_VERSION=1.0.0`

##### 5.4.3 Configuration
- **`capabilities/compiler.yaml`**:
  ```yaml
  enabled: true
  dependencies:
    model:
      id: granite-guardian-3.1-8b
      required: true
    external_tool:
      name: mellea-compiler
      check_command: mellea --version
  input_path: ~/.config/granite-cli/skills/raw
  output_path: ~/.config/granite-cli/skills/compiled
  compiler_options:
    optimization_level: 2
    enable_guardian: true
    strict_mode: true
  ```

##### 5.4.4 Documentation
- Document the compiler abstraction in ARCHITECTURE.md
- Explain what the full implementation should include:
  - Skill definition parsing
  - Mellea compiler invocation
  - Granite Guardian integration for security
  - Compiled artifact generation
- Provide extension points for future implementation

##### 5.4.5 Testing
- Unit tests for abstraction interface
- Integration test: Compiler capability setup completes
- Integration test: Placeholder logic executes without errors
- Manual test: Configuration is saved correctly

**Phase 5 Deliverables:**
- Full implementation of Docling capability
- Full implementation of Vision capability
- Full implementation of Speech capability
- Abstraction placeholder for Compiler capability
- All capabilities integrated with tool adapters
- Comprehensive testing for each capability

---

## 6. Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Tool Overlays** | Non-invasive env vars (subprocess only) | Zero collision risk. Direct tool usage (`claude`) is never affected unless user explicitly exports. |
| **Dependency System** | Lazy DI Factory (depth-first) with cycle detection | Keeps the codebase simple. Commands just declare what they need; the factory handles missing pieces automatically. Cycle detection prevents invalid configurations. |
| **Provider Contract** | Standard APIs only (OpenAI, Ollama, Anthropic) | Allows granite-cli to work with any tool that speaks these, without requiring tools to implement granite-specific logic. |
| **Registries** | Static/Bundled (MVP) | Simple, fast, no network dependency. Can be extended to remote fetch later. |
| **Tool Adapters** | Hardcoded in binary (MVP), version-aware | The primary value prop is automating the setup for known tools. Version awareness handles tool evolution. Extensibility via external plugins is future work. |
| **Chatbot** | REPL + Web Fetch (MVP), may be removed | Skips the complexity of web search. Web fetch provides explicit user control over context injection. May be removed as it's the only part that uses AI. |
| **Capability Architecture** | Execution hooks pattern (not fixed types) | More flexible than fixed capability "types". Capabilities implement only the hooks they need (on_setup, on_configure, on_pre_launch, on_post_launch, on_shutdown, runtime_bindings). |
| **Capability Dependencies** | Support capability-to-capability dependencies | Allows composition (e.g., document-qa depends on docling). Cycle detection prevents circular dependencies. |
| **Configuration Philosophy** | Single-instance semantics with content-hash caching | Each configured item has one canonical configuration. Content hash used as cache key in DI factory. Separate files allow independent configuration. |
| **Model Variants** | Hierarchical (family/version/size/format/precision) with ModelRecommender | Handles complexity of model selection. Recommender considers provider capabilities and hardware profile. Implementation deferred but architecture in place. |
| **Provider Failover** | At launch time only, no proxy | Tests providers in priority order before tool starts. Tool communicates directly with selected provider. No persistent daemon. |
| **Tool Adapter Lifecycle** | Delegated to adapters, no persistent daemon | Each adapter manages its own lifecycle concerns. Granite-cli pushes lifecycle to upstream providers and downstream tools. |
| **Skill Binding** | Tool-specific format conversion via adapters | Adapters convert compiled Mellea skills into tool's native format (JSON, Python modules, etc.). Skills invoked by tool, not granite-cli. |
| **Auto-configuration** | Intelligent defaults with user override | Factory auto-configures when possible but always presents values for review/modification. Critical for model variant selection. |

---

## 7. Verification

After implementation:

### Phase 1 Verification
1. `cargo test` — all unit and integration tests pass
2. `model list` — displays bundled models correctly
3. `model info <model>` — shows detailed model information
4. `capability list` — displays bundled capabilities
5. `capability info <capability>` — shows capability metadata and dependencies
6. Config directory created on first run
7. Shell detection works for bash, zsh, fish

### Phase 2 Verification
1. `provider list` — displays configured providers
2. `provider setup <provider>` — wizard completes successfully
3. `provider health` — checks provider health correctly
4. `model setup <model>` — full wizard with ModelRecommender suggestions
5. `capability setup <capability>` — resolves dependencies in topological order
6. Cycle detection prevents circular capability dependencies
7. `run <model>` — chatbot connects to provider, streams response
8. Web fetch in chatbot converts HTML to markdown
9. Factory caches resolved instances by content hash
10. Auto-configuration presents defaults for user review

### Phase 3 Verification
1. `configure <tool>` — wizard completes successfully
2. Provider-tool matching identifies compatible providers
3. Capability integration: hooks execute in correct order
4. `launch <tool>` — subprocess receives overlay env vars
5. Tool adapter version detection works correctly
6. Skills converted to tool-specific format
7. `configure <tool> --export` — env vars written to shell profile
8. `configure <tool> --reset` — removes tool configuration
9. `launch --dry-run` — shows overlay without launching
10. Direct tool usage (`claude`) remains unaffected by overlays
11. Provider failover selects working provider at launch time

### Phase 4 Verification
1. `run --tui` — TUI chat interface works correctly
2. Markdown rendering with syntax highlighting
3. `model setup --tui` — TUI wizard provides good UX
4. `configure <tool> --tui` — TUI wizard with real-time validation
5. Priority routing selects providers in correct order
6. Health checks detect provider issues
7. Error messages are clear and actionable
8. Logging captures relevant information
9. Documentation is comprehensive and accurate

### Phase 5 Verification
1. Docling capability: skill definition generated correctly
2. Vision capability: model resolved, skill available
3. Speech capability: model resolved, skills available
4. Compiler capability: abstraction interface works
5. All capabilities integrate with tool adapters
6. Tools can invoke capability skills successfully
7. Background services (if any) start and stop cleanly
8. Capability configurations saved correctly

---

## 8. Future Enhancements (Post-MVP)

- **Remote Registries**: Fetch model/provider/capability definitions from remote sources
- **Plugin System**: Support external tool adapters and capabilities
- **Model Caching**: Local caching of model weights for faster startup
- **Advanced Routing**: Load balancing, cost optimization, latency-based selection
- **Telemetry**: Usage analytics and performance monitoring
- **GUI**: Desktop application for non-CLI users
- **Cloud Integration**: Direct integration with cloud providers (AWS, Azure, GCP)
- **Skill Marketplace**: Community-contributed skills and capabilities
- **Full Mellea Compiler**: Complete implementation of skills compiler capability
- **Multi-tool Orchestration**: Coordinate multiple tools in workflows
- **Capability Composition**: Higher-order capabilities that combine multiple capabilities
