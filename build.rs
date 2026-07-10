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

    // Variants - use static slice
    s.push_str("    fn variants(&self) -> &[crate::models::base::ModelVariant] {\n");
    s.push_str("        static VARIANTS: std::sync::LazyLock<Vec<crate::models::base::ModelVariant>> = std::sync::LazyLock::new(|| vec![\n");
    for variant in &model.variants {
        s.push_str("            crate::models::base::ModelVariant {\n");
        s.push_str(&format!("                format: {:?}.to_string(),\n", variant.format));
        s.push_str(&format!("                precision: {:?}.to_string(),\n", variant.precision));
        s.push_str(&format!("                size_gb: {},\n", format_float(variant.size_gb)));
        s.push_str(&format!("                url: {:?}.to_string(),\n", variant.url));
        s.push_str("            },\n");
    }
    s.push_str("        ]);\n");
        s.push_str("        &VARIANTS\n");
    s.push_str("    }\n");

    // Description
    s.push_str("    fn description(&self) -> Option<&str> {\n");
    if let Some(ref desc) = model.description {
        s.push_str(&format!("        Some({:?})\n", desc));
    } else {
        s.push_str("        None\n");
    }
    s.push_str("    }\n");

    // Tags - use static slice
    s.push_str("    fn tags(&self) -> &[String] {\n");
    s.push_str("        static TAGS: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| vec![\n");
    for tag in &model.tags {
        s.push_str(&format!("            {:?}.to_string(),\n", tag));
    }
    s.push_str("        ]);\n");
    s.push_str("        &TAGS\n");
    s.push_str("    }\n");

    // Supported functions
    s.push_str("    fn supported_functions(&self) -> &[crate::models::base::ModelFunction] {\n");
    s.push_str("        static FUNCS: std::sync::LazyLock<Vec<crate::models::base::ModelFunction>> = std::sync::LazyLock::new(|| vec![\n");
    for func in &model.supported_functions {
        s.push_str(&format!("            crate::models::base::ModelFunction::{},\n", func));
    }
    s.push_str("        ]);\n");
    s.push_str("        &FUNCS\n");
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

    // Variants
    s.push_str("            variants: vec![\n");
    for variant in &model.variants {
        s.push_str("                crate::models::base::ModelVariant {\n");
        s.push_str(&format!("                    format: {:?}.to_string(),\n", variant.format));
        s.push_str(&format!("                    precision: {:?}.to_string(),\n", variant.precision));
        s.push_str(&format!("                    size_gb: {},\n", format_float(variant.size_gb)));
        s.push_str(&format!("                    url: {:?}.to_string(),\n", variant.url));
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

    // Supported functions
    s.push_str("            supported_functions: vec![\n");
    for func in &model.supported_functions {
        s.push_str(&format!("                crate::models::base::ModelFunction::{},\n", func));
    }
    s.push_str("            ],\n");

    s.push_str("        }\n");
    s
}

fn format_float(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{:.1}", value)
    } else {
        value.to_string()
    }
}

fn model_id_to_struct_name(id: &str) -> String {
    // Convert "granite-3.1-3b-instruct" to "Granite313bInstruct"
    id.split('-')
        .map(|part| {
            if part.contains('.') {
                // Remove dots: "3.1" -> "31"
                part.replace('.', "")
            } else {
                // Capitalize first letter: "granite" -> "Granite"
                let mut chars = part.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => {
                        first.to_uppercase().collect::<String>() + chars.as_str()
                    }
                }
            }
        })
        .collect::<String>() // Join without separator for proper CamelCase
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
    variants: Vec<YamlModelVariant>,
    description: Option<String>,
    tags: Vec<String>,
    supported_functions: Vec<String>,
}

#[derive(serde::Deserialize)]
struct YamlModelVariant {
    format: String,
    precision: String,
    size_gb: f64,
    url: String,
}