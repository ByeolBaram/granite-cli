# Registry Refactoring Plan

## Overview

This plan refactors the registry system to eliminate duplication and establish single sources of truth for models, capabilities, and providers. The goal is to make the system more flexible and maintainable by enabling self-registration and removing hard-coded definitions.

## Problem Statement

Current issues with the registry implementation:

1. **Models Registry** (`src/registry/models.rs`):
   - Hard-coded model definitions duplicate data from `resources/models.yaml`
   - Adding new models requires editing both files

2. **Capabilities Registry** (`src/registry/capabilities.rs`):
   - Hard-coded capability definitions
   - Metadata duplicated in individual capability implementations (`src/capabilities/*.rs`)
   - Factory logic duplicated in DI layer (`src/di/mod.rs`)

3. **Providers Registry** (`src/registry/providers.rs`):
   - Hard-coded provider definitions
   - Metadata duplicated in individual provider implementations (`src/providers/*.rs`)
   - Factory logic duplicated in DI layer (`src/di/mod.rs`)

## Goals

1. **Single Source of Truth**: Each registry type has exactly one authoritative source
2. **Self-Registration**: Implementations register themselves without central coordination
3. **Type Safety**: Maintain compile-time type safety with type-specific factory configs
4. **Minimal DI Changes**: Defer full DI overhaul, make minimal changes for now
5. **Remove Duplication**: Eliminate all hard-coded registry definitions

## Architecture Design

### 1. Models Registry - Build-Time Loading

**Source of Truth**: `resources/models.yaml`

**Approach**: Use `build.rs` to generate Rust code from YAML at compile time

**Benefits**:
- No runtime YAML parsing overhead
- Single source of truth
- Type-safe at compile time
- Easy to add new models (just edit YAML)

**Implementation**:

```rust
// build.rs
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=resources/models.yaml");
    
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("generated_models.rs");
    
    // Read and parse YAML
    let yaml_content = fs::read_to_string("resources/models.yaml")
        .expect("Failed to read models.yaml");
    
    let models: Vec<ModelDefinition> = serde_yaml::from_str(&yaml_content)
        .expect("Failed to parse models.yaml");
    
    // Generate Rust code
    let code = generate_models_code(&models);
    
    fs::write(&dest_path, code)
        .expect("Failed to write generated code");
}

fn generate_models_code(models: &[ModelDefinition]) -> String {
    let mut code = String::from("// Auto-generated from models.yaml\n\n");
    code.push_str("pub static MODELS: &[ModelDefinition] = &[\n");
    
    for model in models {
        code.push_str(&format!("    {},\n", generate_model_literal(model)));
    }
    
    code.push_str("];\n");
    code
}
```

```rust
// src/registry/models.rs
use once_cell::sync::Lazy;
use std::collections::HashMap;

// Include generated code
include!(concat!(env!("OUT_DIR"), "/generated_models.rs"));

pub static MODEL_REGISTRY: Lazy<HashMap<String, &'static ModelDefinition>> = Lazy::new(|| {
    let mut map = HashMap::new();
    for model in MODELS {
        map.insert(model.id.clone(), model);
    }
    map
});

impl Registry<ModelDefinition> for ModelRegistry {
    fn list(&self) -> Vec<&ModelDefinition> {
        MODELS.iter().collect()
    }
    
    fn get(&self, id: &str) -> Option<&ModelDefinition> {
        MODEL_REGISTRY.get(id).copied()
    }
    
    fn search(&self, query: &str) -> Vec<&ModelDefinition> {
        let query_lower = query.to_lowercase();
        MODELS.iter()
            .filter(|m| {
                m.id.to_lowercase().contains(&query_lower)
                    || m.family.to_lowercase().contains(&query_lower)
                    || m.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
}
```

### 2. Capabilities Registry - Self-Registration

**Source of Truth**: Individual capability implementations

**Approach**: Use `inventory` crate for compile-time self-registration with type-safe factory functions

**Benefits**:
- Each capability owns its metadata
- No central coordination needed
- Type-safe factory configs
- Automatic discovery

**Implementation**:

```rust
// src/capabilities/mod.rs

use inventory;
use std::collections::HashMap;
use once_cell::sync::Lazy;

/// Trait for capability registration with type-safe config
pub trait CapabilityRegistration: Capability {
    /// The configuration type specific to this capability
    type Config: serde::de::DeserializeOwned;
    
    /// Metadata for registry
    fn metadata() -> CapabilityDefinition where Self: Sized;
    
    /// Factory function to create instances
    fn create(config: &Self::Config) -> Result<Box<dyn Capability>> where Self: Sized;
}

/// Factory wrapper for inventory collection
pub struct CapabilityFactory {
    pub metadata: CapabilityDefinition,
    pub create: fn(&serde_json::Value) -> Result<Box<dyn Capability>>,
}

// Collect all registered capabilities
inventory::collect!(CapabilityFactory);

/// Global capability registry
pub static CAPABILITY_REGISTRY: Lazy<HashMap<String, &'static CapabilityFactory>> = Lazy::new(|| {
    let mut map = HashMap::new();
    for factory in inventory::iter::<CapabilityFactory> {
        map.insert(factory.metadata.id.clone(), factory);
    }
    map
});

/// Macro to simplify registration
#[macro_export]
macro_rules! register_capability {
    ($cap_type:ty) => {
        inventory::submit! {
            CapabilityFactory {
                metadata: <$cap_type as CapabilityRegistration>::metadata(),
                create: |config| {
                    let typed_config = serde_json::from_value(config.clone())?;
                    <$cap_type as CapabilityRegistration>::create(&typed_config)
                },
            }
        }
    };
}
```

```rust
// src/capabilities/docling.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct DoclingConfig {
    // Docling-specific config fields
    pub enabled: bool,
}

impl CapabilityRegistration for DoclingCapability {
    type Config = DoclingConfig;
    
    fn metadata() -> CapabilityDefinition {
        CapabilityDefinition {
            id: "docling".to_string(),
            name: "Document Conversion".to_string(),
            description: "Convert various document formats (PDF, DOCX, PPTX, XLSX) to markdown using IBM Docling.".to_string(),
            version: "0.1.0".to_string(),
            dependencies: vec![
                Dependency::ExternalTool {
                    name: "docling".to_string(),
                    check_command: "python -c \"import docling\"".to_string(),
                },
            ],
            hooks: vec!["on_setup".to_string(), "runtime_bindings".to_string()],
            tags: vec!["document".to_string(), "conversion".to_string(), "markdown".to_string()],
        }
    }
    
    fn create(config: &DoclingConfig) -> Result<Box<dyn Capability>> {
        Ok(Box::new(DoclingCapability {
            id: "docling".to_string(),
            name: "Document Conversion".to_string(),
            description: "Convert various document formats...".to_string(),
            dependencies: vec![],
            enabled: config.enabled,
        }))
    }
}

// Self-register
register_capability!(DoclingCapability);
```

### 3. Providers Registry - Self-Registration

**Source of Truth**: Individual provider implementations

**Approach**: Same as capabilities - `inventory` with type-safe factory functions

**Implementation**:

```rust
// src/providers/mod.rs

use inventory;
use std::collections::HashMap;
use once_cell::sync::Lazy;

/// Trait for provider registration with type-safe config
pub trait ProviderRegistration: Provider {
    /// The configuration type specific to this provider
    type Config: serde::de::DeserializeOwned;
    
    /// Metadata for registry
    fn metadata() -> ProviderDefinition where Self: Sized;
    
    /// Factory function to create instances
    fn create(config: &Self::Config) -> Result<Box<dyn Provider>> where Self: Sized;
}

/// Factory wrapper for inventory collection
pub struct ProviderFactory {
    pub metadata: ProviderDefinition,
    pub create: fn(&serde_json::Value) -> Result<Box<dyn Provider>>,
}

// Collect all registered providers
inventory::collect!(ProviderFactory);

/// Global provider registry
pub static PROVIDER_REGISTRY: Lazy<HashMap<String, &'static ProviderFactory>> = Lazy::new(|| {
    let mut map = HashMap::new();
    for factory in inventory::iter::<ProviderFactory> {
        map.insert(factory.metadata.id.clone(), factory);
    }
    map
});

/// Macro to simplify registration
#[macro_export]
macro_rules! register_provider {
    ($provider_type:ty) => {
        inventory::submit! {
            ProviderFactory {
                metadata: <$provider_type as ProviderRegistration>::metadata(),
                create: |config| {
                    let typed_config = serde_json::from_value(config.clone())?;
                    <$provider_type as ProviderRegistration>::create(&typed_config)
                },
            }
        }
    };
}
```

```rust
// src/providers/ollama.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct OllamaConfig {
    pub endpoint: String,
    // Ollama-specific config fields
}

impl ProviderRegistration for OllamaProvider {
    type Config = OllamaConfig;
    
    fn metadata() -> ProviderDefinition {
        ProviderDefinition {
            id: "ollama".to_string(),
            name: "Ollama".to_string(),
            description: "Local model serving via Ollama. Supports many open-source models.".to_string(),
            provider_type: ProviderType::Local,
            default_endpoint: "http://localhost:11434".to_string(),
            api_capabilities: vec![ApiSurface::OllamaChat],
            supported_formats: vec![ModelFormat::GGUF],
            supported_precisions: vec![
                Precision::Q8_0,
                Precision::Q4_K_M,
                Precision::Q5_K_M,
                Precision::Q3_K_M,
            ],
            authentication: vec![AuthType::None],
            tags: vec!["ollama".to_string(), "local".to_string(), "gguf".to_string()],
        }
    }
    
    fn create(config: &OllamaConfig) -> Result<Box<dyn Provider>> {
        Ok(Box::new(OllamaProvider::new(
            "ollama",
            "Ollama",
            config.endpoint.clone(),
        )))
    }
}

// Self-register
register_provider!(OllamaProvider);
```

### 4. DI Layer - Minimal Changes

For now, make minimal changes to the DI layer. Full overhaul will come later.

**Changes**:
1. Look up factories from registry instead of hard-coded match
2. Add simple adapter to convert `ProviderConfig` to type-specific config JSON
3. Remove hard-coded factory logic

```rust
// src/di/mod.rs

impl Factory {
    fn create_provider_from_config(&self, config: &ProviderConfig) -> Result<Box<dyn Provider>> {
        // Look up factory from registry
        let factory = PROVIDER_REGISTRY
            .get(&config.provider_id)
            .ok_or_else(|| anyhow::anyhow!("Provider '{}' not found in registry", config.provider_id))?;
        
        // Convert ProviderConfig to JSON for type-specific parsing
        let config_json = serde_json::to_value(config)?;
        
        // Use factory to create instance
        (factory.create)(&config_json)
    }
    
    fn resolve_capability_registry(&self, id: &str) -> Result<Box<dyn Capability>> {
        // Look up factory from registry
        let factory = CAPABILITY_REGISTRY
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Capability '{}' not found in registry", id))?;
        
        // Create with default/empty config for now
        let config_json = serde_json::json!({});
        
        // Use factory to create instance
        (factory.create)(&config_json)
    }
}
```

## Implementation Steps

### Phase 1: Setup and Dependencies

**Tasks**:
1. Add `inventory` crate to `Cargo.toml`
2. Add `serde_yaml` to `[build-dependencies]` in `Cargo.toml`
3. Verify `once_cell` is available (should already be present)

**Files Modified**:
- `Cargo.toml`

**Estimated Time**: 15 minutes

### Phase 2: Models Registry Refactor

**Tasks**:
1. Create `build.rs` in project root
2. Implement YAML parsing and code generation
3. Update `src/registry/models.rs` to use generated code
4. Remove hard-coded model definitions
5. Update tests to work with generated data
6. Verify `cargo build` works and models are accessible

**Files Modified**:
- `build.rs` (new)
- `src/registry/models.rs`
- `src/registry/mod.rs` (if needed)

**Files Removed**:
- Hard-coded model definitions in `src/registry/models.rs`

**Estimated Time**: 3-4 hours

**Success Criteria**:
- [ ] `cargo build` succeeds
- [ ] Models loaded from YAML at build time
- [ ] All existing tests pass
- [ ] `MODEL_REGISTRY.get()` works as before
- [ ] No hard-coded model definitions remain

### Phase 3: Providers Registry Refactor

**Tasks**:
1. Add `ProviderRegistration` trait to `src/providers/mod.rs`
2. Create `ProviderFactory` struct and inventory collection
3. Create `register_provider!` macro
4. Implement `ProviderRegistration` for `OllamaProvider`
5. Implement `ProviderRegistration` for `OpenAiCompatProvider`
6. Implement `ProviderRegistration` for `AnthropicProvider`
7. Update `PROVIDER_REGISTRY` to use inventory
8. Remove hard-coded provider definitions from `src/registry/providers.rs`
9. Update DI layer to use registry factories
10. Update all tests

**Files Modified**:
- `src/providers/mod.rs`
- `src/providers/ollama.rs`
- `src/providers/openai_compat.rs`
- `src/providers/anthropic.rs`
- `src/registry/providers.rs`
- `src/di/mod.rs`

**Files Removed**:
- Hard-coded provider definitions in `src/registry/providers.rs`

**Estimated Time**: 4-5 hours

**Success Criteria**:
- [ ] All providers self-register
- [ ] `PROVIDER_REGISTRY` populated from inventory
- [ ] DI layer uses registry factories
- [ ] All existing tests pass
- [ ] No hard-coded provider definitions remain
- [ ] Can create provider instances via registry

### Phase 4: Capabilities Registry Refactor

**Tasks**:
1. Add `CapabilityRegistration` trait to `src/capabilities/mod.rs`
2. Create `CapabilityFactory` struct and inventory collection
3. Create `register_capability!` macro
4. Implement `CapabilityRegistration` for `DoclingCapability`
5. Implement `CapabilityRegistration` for `VisionCapability`
6. Implement `CapabilityRegistration` for `SpeechCapability`
7. Implement `CapabilityRegistration` for `CompilerCapability`
8. Update `CAPABILITY_REGISTRY` to use inventory
9. Remove hard-coded capability definitions from `src/registry/capabilities.rs`
10. Update DI layer to use registry factories
11. Update all tests

**Files Modified**:
- `src/capabilities/mod.rs`
- `src/capabilities/docling.rs`
- `src/capabilities/vision.rs`
- `src/capabilities/speech.rs`
- `src/capabilities/compiler.rs`
- `src/registry/capabilities.rs`
- `src/di/mod.rs`

**Files Removed**:
- Hard-coded capability definitions in `src/registry/capabilities.rs`

**Estimated Time**: 4-5 hours

**Success Criteria**:
- [ ] All capabilities self-register
- [ ] `CAPABILITY_REGISTRY` populated from inventory
- [ ] DI layer uses registry factories
- [ ] All existing tests pass
- [ ] No hard-coded capability definitions remain
- [ ] Can create capability instances via registry

### Phase 5: Testing and Validation

**Tasks**:
1. Run full test suite (`cargo test`)
2. Test CLI commands manually
3. Verify model listing works
4. Verify provider listing works
5. Verify capability listing works
6. Test provider instantiation
7. Test capability instantiation
8. Check for any regressions

**Estimated Time**: 2-3 hours

**Success Criteria**:
- [ ] All tests pass
- [ ] No regressions in functionality
- [ ] CLI commands work as expected
- [ ] Can list all models/providers/capabilities
- [ ] Can create instances successfully

### Phase 6: Documentation

**Tasks**:
1. Update `CONTRIBUTING.md` with new patterns
2. Document how to add new models (edit YAML)
3. Document how to add new capabilities (implement trait + register)
4. Document how to add new providers (implement trait + register)
5. Update `README.md` if needed
6. Add inline code documentation

**Files Modified**:
- `CONTRIBUTING.md`
- `README.md` (if needed)
- Various source files (doc comments)

**Estimated Time**: 2-3 hours

**Success Criteria**:
- [ ] Clear documentation for adding models
- [ ] Clear documentation for adding capabilities
- [ ] Clear documentation for adding providers
- [ ] Examples provided
- [ ] Architecture explained

## Dependencies to Add

```toml
[dependencies]
inventory = "0.3"
once_cell = "1.19"  # May already be present
serde_json = "1"    # May already be present

[build-dependencies]
serde_yaml = "0.9"
```

## Migration Strategy

### Breaking Changes

**None for external users** - all public APIs remain the same:
- `MODEL_REGISTRY.get(id)` still works
- `PROVIDER_REGISTRY.get(id)` still works
- `CAPABILITY_REGISTRY.get(id)` still works

**Internal changes only**:
- Registry implementations change
- Factory creation moves to implementations
- DI layer simplified

### Rollout Plan

1. **Phase 1-2**: Low risk, can be done independently
2. **Phase 3**: Medium risk, test provider creation thoroughly
3. **Phase 4**: Medium risk, test capability creation thoroughly
4. **Phase 5**: Critical validation phase
5. **Phase 6**: Documentation updates

### Testing Strategy

- Unit tests for each registry type
- Integration tests for DI layer
- Manual CLI testing
- Regression testing for all features

### Rollback Strategy

- Each phase is in a separate commit
- Can rollback individual phases if needed
- Comprehensive testing before merging
- Feature branch for all changes

## Benefits

1. **Single Source of Truth**: No more duplication
2. **Self-Registration**: Implementations own their metadata
3. **Type Safety**: Compile-time guarantees with associated types
4. **Easier to Extend**: Add new items by implementing traits
5. **Maintainability**: Less code, clearer responsibilities
6. **Flexibility**: Easy to add new registry types
7. **Performance**: Build-time loading for models

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Build script complexity | Medium | Keep simple, add error handling |
| Inventory crate issues | Low | Well-maintained, widely used |
| Type safety complexity | Medium | Good documentation, examples |
| Test coverage gaps | High | Comprehensive testing in Phase 5 |
| Breaking existing code | High | Preserve APIs, thorough testing |

## Success Criteria

- [ ] All tests pass
- [ ] No API breaking changes
- [ ] Models loaded from YAML at build time
- [ ] Capabilities self-register via traits
- [ ] Providers self-register via traits
- [ ] DI layer uses registries, no hard-coded logic
- [ ] All hard-coded registry definitions removed
- [ ] Documentation updated
- [ ] Easy to add new models/capabilities/providers

## Timeline Estimate

- Phase 1: 15 minutes
- Phase 2: 3-4 hours
- Phase 3: 4-5 hours
- Phase 4: 4-5 hours
- Phase 5: 2-3 hours
- Phase 6: 2-3 hours

**Total**: 16-20 hours of development time

## Next Steps

1. Review this plan
2. Get approval for approach
3. Create feature branch: `feature/registry-refactor`
4. Begin Phase 1 implementation
5. Iterate through phases with testing at each step
6. Merge to main after Phase 6 complete
