# Model Registry Implementation Plan

## Current State Analysis

### Factory System
The project uses a sophisticated factory pattern defined in `src/registry/mod.rs`:
- `define_factory!` macro creates registration infrastructure
- Requires implementations to provide:
  - `ConfigConstructable` trait: `fn new(cfg: &serde_json::Value) -> Self`
  - `HasModelMetadata` trait: `fn metadata() -> ModelMetadata`
- Factory maintains a registry of type metadata, not instances
- Factory can construct instances on-demand from configuration

### Current Model Structure
- `src/models/base.rs` defines:
  - `Model` trait (the interface)
  - `ModelMetadata` struct (describes a model)
  - `ModelType` enum
  - `ModelVariant` struct with String fields for format and precision
  - `ModelFactory` (via `define_factory!` macro)
- `src/models/mod.rs` creates empty `MODEL_REGISTRY`
- `resources/models.yaml` contains 7 model definitions

### Current Build Script Issues
The existing `build.rs`:
1. Tries to generate model literals directly
2. Doesn't match the factory registration pattern
3. References non-existent types (`ModelFormat`, `Precision`)
4. Generates code that won't compile with current factory system

## Implementation Plan

### Step 1: Create Concrete Model Implementation

Create a `StaticModel` struct in `src/models/base.rs` that:
- Holds all model data from YAML
- Implements `Model` trait
- Implements `ConfigConstructable` (even though config is ignored for static models)
- Implements `HasModelMetadata` to return its metadata

```rust
pub struct StaticModel {
    metadata: ModelMetadata,
}

impl Model for StaticModel {
    fn id(&self) -> &str { &self.metadata.id }
    fn family(&self) -> &str { &self.metadata.family }
    fn version(&self) -> &str { &self.metadata.version }
    fn size(&self) -> u64 { self.metadata.size }
    fn context_length(&self) -> u64 { self.metadata.context_length }
    fn model_type(&self) -> &ModelType { &self.metadata.model_type }
    fn huggingface_repo(&self) -> &str { &self.metadata.huggingface_repo }
    fn required_provider_capabilities(&self) -> &[String] { 
        &self.metadata.required_provider_capabilities 
    }
    fn variants(&self) -> &[ModelVariant] { &self.metadata.variants }
    fn description(&self) -> Option<&str> { 
        self.metadata.description.as_deref() 
    }
    fn tags(&self) -> &[String] { &self.metadata.tags }
}

impl ConfigConstructable for StaticModel {
    fn new(_cfg: &serde_json::Value) -> Self {
        // Static models don't use config, but must implement trait
        panic!("StaticModel should not be constructed via factory")
    }
}

impl HasModelMetadata for StaticModel {
    fn metadata() -> ModelMetadata {
        // This will be overridden per-model in generated code
        unimplemented!()
    }
}
```

### Step 2: Add ModelType From Implementation

Add a `From<&str>` implementation for `ModelType` in `src/models/base.rs` to support parsing from YAML strings:

```rust
impl From<&str> for ModelType {
    fn from(s: &str) -> Self {
        match s {
            "Text" => ModelType::Text,
            "Vision" => ModelType::Vision,
            "Speech" => ModelType::Speech,
            "Embedding" => ModelType::Embedding,
            _ => panic!("Unknown model type: {}", s),
        }
    }
}
```

### Step 3: Update Build Script

Rewrite `build.rs` to generate code that:
1. Creates a unique struct for each model (e.g., `Granite31_3bInstruct`)
2. Each struct implements `HasModelMetadata` with its specific data
3. Generates a registration function that registers all models

Key changes from old build.rs:
- Remove references to `ModelFormat` and `Precision` enums
- Keep `format` and `precision` as Strings
- Generate per-model structs instead of literals
- Generate registration function

```rust
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=resources/models.yaml");

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("generated_models.rs");

    let yaml_content =
        fs::read_to_string("resources/models.yaml").expect("Failed to read models.yaml");

    let models: Vec<YamlModel> =
        serde_yaml::from_str(&yaml_content).expect("Failed to parse models.yaml");

    let code = generate_models_code(&models);

    fs::write(&dest_path, code).expect("Failed to write generated code");
}

fn generate_models_code(models: &[YamlModel]) -> String {
    let mut code = String::from(
        "// Auto-generated from resources/models.yaml - do not edit\n\n",
    );
    
    // Generate a struct for each model
    for model in models {
        code.push_str(&generate_model_struct(model));
    }
    
    // Generate registration function
    code.push_str("pub fn register_all_models(factory: &mut crate::models::base::ModelFactory) {\n");
    for model in models {
        let struct_name = model_id_to_struct_name(&model.id);
        code.push_str(&format!("    factory.register::<{}>(\"{}\");\n", struct_name, model.id));
    }
    code.push_str("}\n");
    
    code
}

fn generate_model_struct(model: &YamlModel) -> String {
    let struct_name = model_id_to_struct_name(&model.id);
    let mut s = String::new();
    
    // Empty struct
    s.push_str(&format!("pub struct {} {{}}\n\n", struct_name));
    
    // ConfigConstructable implementation
    s.push_str(&format!("impl crate::registry::ConfigConstructable for {} {{\n", struct_name));
    s.push_str("    fn new(_cfg: &serde_json::Value) -> Self { Self {} }\n");
    s.push_str("}\n\n");
    
    // Model trait implementation
    s.push_str(&format!("impl crate::models::base::Model for {} {{\n", struct_name));
    s.push_str(&format!("    fn id(&self) -> &str {{ {:?} }}\n", model.id));
    s.push_str(&format!("    fn family(&self) -> &str {{ {:?} }}\n", model.family));
    s.push_str(&format!("    fn version(&self) -> &str {{ {:?} }}\n", model.version));
    s.push_str(&format!("    fn size(&self) -> u64 {{ {} }}\n", model.size));
    s.push_str(&format!("    fn context_length(&self) -> u64 {{ {} }}\n", model.context_length));
    s.push_str(&format!("    fn model_type(&self) -> &crate::models::base::ModelType {{ &crate::models::base::ModelType::{} }}\n", model.model_type));
    s.push_str(&format!("    fn huggingface_repo(&self) -> &str {{ {:?} }}\n", model.huggingface_repo));
    s.push_str("    fn required_provider_capabilities(&self) -> &[String] {\n");
    s.push_str("        &[]\n"); // Will be filled with static data
    s.push_str("    }\n");
    s.push_str("    fn variants(&self) -> &[crate::models::base::ModelVariant] {\n");
    s.push_str("        &[]\n"); // Will be filled with static data
    s.push_str("    }\n");
    s.push_str("    fn description(&self) -> Option<&str> {\n");
    if let Some(ref desc) = model.description {
        s.push_str(&format!("        Some({:?})\n", desc));
    } else {
        s.push_str("        None\n");
    }
    s.push_str("    }\n");
    s.push_str("    fn tags(&self) -> &[String] {\n");
    s.push_str("        &[]\n"); // Will be filled with static data
    s.push_str("    }\n");
    s.push_str("}\n\n");
    
    // HasModelMetadata implementation
    s.push_str(&format!("impl crate::models::base::HasModelMetadata for {} {{\n", struct_name));
    s.push_str("    fn metadata() -> crate::models::base::ModelMetadata {\n");
    s.push_str(&generate_metadata_literal(model));
    s.push_str("    }\n");
    s.push_str("}\n\n");
    
    s
}

fn generate_metadata_literal(model: &YamlModel) -> String {
    let mut s = String::new();
    s.push_str("        crate::models::base::ModelMetadata {\n");
    s.push_str(&format!("            id: {:?}.to_string(),\n", model.id));
    s.push_str(&format!("            family: {:?}.to_string(),\n", model.family));
    s.push_str(&format!("            version: {:?}.to_string(),\n", model.version));
    s.push_str(&format!("            size: {},\n", model.size));
    s.push_str(&format!("            context_length: {},\n", model.context_length));
    s.push_str(&format!("            model_type: crate::models::base::ModelType::{},\n", model.model_type));
    s.push_str(&format!("            huggingface_repo: {:?}.to_string(),\n", model.huggingface_repo));
    
    // Required provider capabilities
    s.push_str("            required_provider_capabilities: vec![\n");
    for cap in &model.required_provider_capabilities {
        s.push_str(&format!("                {:?}.to_string(),\n", cap));
    }
    s.push_str("            ],\n");
    
    // Variants
    s.push_str("            variants: vec![\n");
    for variant in &model.variants {
        s.push_str("                crate::models::base::ModelVariant {\n");
        s.push_str(&format!("                    format: {:?}.to_string(),\n", variant.format));
        s.push_str(&format!("                    precision: {:?}.to_string(),\n", variant.precision));
        s.push_str(&format!("                    size_gb: {},\n", variant.size_gb));
        s.push_str(&format!("                    huggingface_path: {:?}.to_string(),\n", variant.huggingface_path));
        s.push_str("                },\n");
    }
    s.push_str("            ],\n");
    
    // Description
    if let Some(ref desc) = model.description {
        s.push_str(&format!("            description: Some({:?}.to_string()),\n", desc));
    } else {
        s.push_str("            description: None,\n");
    }
    
    // Tags
    s.push_str("            tags: vec![\n");
    for tag in &model.tags {
        s.push_str(&format!("                {:?}.to_string(),\n", tag));
    }
    s.push_str("            ],\n");
    
    s.push_str("        }\n");
    s
}

fn model_id_to_struct_name(id: &str) -> String {
    // Convert "granite-3.1-3b-instruct" to "Granite31_3bInstruct"
    id.split('-')
        .map(|part| {
            if part.contains('.') {
                part.replace('.', "")
            } else {
                let mut chars = part.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => {
                        first.to_uppercase().collect::<String>() + chars.as_str()
                    }
                }
            }
        })
        .collect::<Vec<_>>()
        .join("_")
}

#[derive(serde::Deserialize)]
struct YamlModel {
    id: String,
    family: String,
    version: String,
    size: u64,
    context_length: u64,
    model_type: String,
    huggingface_repo: String,
    required_provider_capabilities: Vec<String>,
    variants: Vec<YamlModelVariant>,
    description: Option<String>,
    tags: Vec<String>,
}

#[derive(serde::Deserialize)]
struct YamlModelVariant {
    format: String,
    precision: String,
    size_gb: f64,
    huggingface_path: String,
}
```

### Step 4: Update Module to Use Generated Code

Update `src/models/mod.rs`:

```rust
use std::sync::LazyLock;

// Include generated code
include!(concat!(env!("OUT_DIR"), "/generated_models.rs"));

pub static MODEL_REGISTRY: LazyLock<base::ModelFactory> = LazyLock::new(|| {
    let mut factory = base::ModelFactory::new();
    register_all_models(&mut factory);
    factory
});

mod base;
pub use base::{Model, ModelMetadata, ModelType, ModelVariant};
```

### Step 5: Testing Strategy

1. **Build Test**: Verify `cargo build` succeeds
2. **Registration Test**: Check that all 7 models are registered
3. **Metadata Test**: Verify metadata can be retrieved for each model
4. **List Test**: Verify `factory.list()` returns all models
5. **Get Test**: Verify `factory.get("granite-3.1-8b-instruct")` returns correct metadata

## Key Design Decisions

### Why Per-Model Structs?
- Factory pattern requires types, not instances
- Each model needs its own `HasModelMetadata` implementation
- Allows compile-time verification of model data
- Zero runtime overhead for metadata access

### Why Keep Strings for Format/Precision?
- Many precision formats exist (Q4_K_M, Q5_K_M, Q8_0, BF16, FP16, etc.)
- Hard-coding enums would require constant updates
- String approach is flexible and extensible
- Validation can happen at runtime if needed

### Why Not Use Config for Static Models?
- Static models are known at compile time
- Config is for runtime-configured instances
- Static models implement `ConfigConstructable` only to satisfy trait bounds
- The `new()` method panics if called (should never happen)

### Why Generate Code Instead of Runtime Parsing?
- Compile-time verification of model data
- No runtime YAML parsing overhead
- Type-safe access to model information
- Follows the factory pattern's design

## Implementation Order

1. ✅ Analyze current system (this document)
2. Add `From<&str>` for `ModelType` to `base.rs`
3. Add `StaticModel` base implementation to `base.rs` (optional, may not be needed)
4. Rewrite `build.rs` to generate per-model structs
5. Update `mod.rs` to include generated code and register models
6. Add tests to verify registration works
7. Update commands to use the populated registry

## Success Criteria

- [ ] `cargo build` succeeds without errors
- [ ] All 7 models from YAML are registered in factory
- [ ] `MODEL_REGISTRY.list()` returns 7 models
- [ ] `MODEL_REGISTRY.get("granite-3.1-8b-instruct")` returns correct metadata
- [ ] Model metadata includes all fields from YAML
- [ ] Variants correctly use String for format and precision
- [ ] No new enum types introduced for format/precision