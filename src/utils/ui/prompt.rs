use serde_json::Value;

use crate::utils::ui::Ui;

/*-- Public entry point --------------------------------------------------------*/

/// Interactively prompt for a config value matching `schema`, pre-filled with
/// `defaults` and editable field-by-field. Recurses into nested objects and
/// arrays (indented), and masks input for fields whose schema is marked
/// `"format": "password"` (see `registry::Secret`) rather than guessing from
/// field names.
pub fn prompt_from_schema(
    ui: &dyn Ui,
    schema: &schemars::Schema,
    defaults: &Value,
) -> anyhow::Result<Value> {
    let root = serde_json::to_value(schema)?;
    prompt_value(ui, &root, &root, defaults, "", "")
}

/*-- Recursive dispatch ---------------------------------------------------------*/

fn prompt_value(
    ui: &dyn Ui,
    root: &Value,
    node: &Value,
    default: &Value,
    indent: &str,
    label: &str,
) -> anyhow::Result<Value> {
    let node = resolve_ref(root, node);
    match get_promptable_type(node).as_deref() {
        Some("object") => prompt_object(ui, root, node, default, indent, label),
        Some("array") => prompt_array(ui, root, node, default, indent, label),
        Some("string") => Ok(prompt_string(ui, node, default, indent, label)?),
        Some("integer") | Some("number") => Ok(prompt_number(ui, node, default, indent, label)?),
        Some("boolean") => Ok(prompt_bool(ui, default, indent, label)?),
        _ => Ok(default.clone()),
    }
}

fn prompt_object(
    ui: &dyn Ui,
    root: &Value,
    node: &Value,
    default: &Value,
    indent: &str,
    label: &str,
) -> anyhow::Result<Value> {
    if !indent.is_empty() && !label.is_empty() {
        ui.info(&format!("{indent}{label}:"));
    }

    let mut result = serde_json::Map::new();
    if let Some(properties) = node.get("properties").and_then(Value::as_object) {
        let child_indent = format!("{indent}  ");
        for (name, prop_schema) in properties {
            let prop_schema = resolve_ref(root, prop_schema);
            if get_promptable_type(prop_schema).is_none() {
                // Untyped / enum-keyed map / unresolved $ref -- no generic UI
                // for these; leave absent so serde falls back to the config
                // struct's own defaults.
                continue;
            }
            let prop_default = default.get(name).cloned().unwrap_or(Value::Null);
            let value = prompt_value(ui, root, prop_schema, &prop_default, &child_indent, name)?;
            result.insert(name.clone(), value);
        }
    }
    Ok(Value::Object(result))
}

fn prompt_array(
    ui: &dyn Ui,
    root: &Value,
    node: &Value,
    default: &Value,
    indent: &str,
    label: &str,
) -> anyhow::Result<Value> {
    let Some(items_schema) = node.get("items").map(|v| resolve_ref(root, v)) else {
        return Ok(Value::Array(vec![]));
    };

    let default_items: Vec<Value> = default.as_array().cloned().unwrap_or_default();
    let mut defaults_iter = default_items.into_iter();
    let child_indent = format!("{indent}  ");
    let mut items = Vec::new();

    loop {
        let next_default = defaults_iter.next();
        let add = ui.confirm(&format!("{indent}Add {label}?"), next_default.is_some())?;
        if !add {
            break;
        }
        let item_default = next_default.unwrap_or_else(|| zero_value_for(items_schema));
        let item = prompt_value(ui, root, items_schema, &item_default, &child_indent, label)?;
        items.push(item);
    }

    Ok(Value::Array(items))
}

/*-- Leaf prompts ---------------------------------------------------------------*/

fn prompt_string(
    ui: &dyn Ui,
    node: &Value,
    default: &Value,
    indent: &str,
    label: &str,
) -> anyhow::Result<Value> {
    let default_str = default.as_str().unwrap_or("").to_string();
    let prompt = format!("{indent}{label}");

    if is_secret_schema(node) {
        let entered = ui.password(&format!("{prompt} (leave blank to keep current)"))?;
        let value = if entered.is_empty() {
            default_str
        } else {
            entered
        };
        Ok(Value::String(value))
    } else {
        let entered = ui.text(&prompt, &default_str)?;
        Ok(Value::String(entered))
    }
}

fn prompt_number(
    ui: &dyn Ui,
    node: &Value,
    default: &Value,
    indent: &str,
    label: &str,
) -> anyhow::Result<Value> {
    let is_integer = get_promptable_type(node).as_deref() == Some("integer");
    let default_str = default
        .as_number()
        .map(|n| n.to_string())
        .unwrap_or_else(|| "0".to_string());

    let entered = ui.text(&format!("{indent}{label}"), &default_str)?;

    let number = if is_integer {
        entered
            .trim()
            .parse::<i64>()
            .map(serde_json::Number::from)
            .map_err(|_| anyhow::anyhow!("'{entered}' is not a valid integer for {label}"))?
    } else {
        entered
            .trim()
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .ok_or_else(|| anyhow::anyhow!("'{entered}' is not a valid number for {label}"))?
    };

    Ok(Value::Number(number))
}

fn prompt_bool(ui: &dyn Ui, default: &Value, indent: &str, label: &str) -> anyhow::Result<Value> {
    let default_bool = default.as_bool().unwrap_or(false);
    let value = ui.confirm(&format!("{indent}{label}"), default_bool)?;
    Ok(Value::Bool(value))
}

/*-- Pure helpers (unit-testable without a terminal) ----------------------------*/

/// True when the schema marks this field as sensitive via `Secret`'s
/// `"format": "password"` marker -- never guessed from a field name.
fn is_secret_schema(node: &Value) -> bool {
    node.get("format").and_then(Value::as_str) == Some("password")
}

/// Extract the promptable type from a schema node. Returns the concrete type
/// string (`"object"`, `"array"`, `"string"`, `"integer"`, `"number"`,
/// `"boolean"`) when the schema represents one, or `None` when it doesn't.
///
/// Handles three schemars representations of `Option<T>`:
/// 1. `"type": "string"` — direct type after `resolve_ref` unwrapping
/// 2. `"type": ["string", "null"]` — type array (some schemars versions)
/// 3. `anyOf` with a single non-null variant — unresolved ref inside anyOf
fn get_promptable_type(node: &Value) -> Option<String> {
    let promptable_types = [
        "object",
        "array",
        "string",
        "integer",
        "number",
        "boolean",
    ];

    // Case 1: `type` is a single string (e.g. `"string"`).
    if let Some(t) = node.get("type").and_then(Value::as_str) {
        if promptable_types.contains(&t) {
            return Some(t.to_string());
        }
    }

    // Case 2: `type` is an array of strings (e.g. `["string", "null"]`).
    if let Some(types) = node.get("type").and_then(Value::as_array) {
        let non_null: Vec<&str> = types
            .iter()
            .filter_map(|v| v.as_str())
            .filter(|t| *t != "null")
            .collect();
        if non_null.len() == 1 && promptable_types.contains(&non_null[0]) {
            return Some(non_null[0].to_string());
        }
    }

    // Case 3: `anyOf` with exactly one non-null variant means schemars
    // represented an Option<T> and resolve_ref may not have fully
    // unwrapped it (e.g. unresolved $ref inside anyOf).
    if let Some(variants) = node.get("anyOf").and_then(Value::as_array) {
        let mut non_null = variants.iter().filter(|v| {
            v.get("type").and_then(Value::as_str) != Some("null")
                && v.get("type").and_then(Value::as_str).is_some()
        });
        if let (Some(only), None) = (non_null.next(), non_null.next()) {
            return get_promptable_type(only);
        }
    }

    None
}

/// Resolve a schema node to the concrete (object/array/scalar) schema it
/// describes, following two indirections schemars commonly introduces:
///
/// - `{"$ref": "#/$defs/Name"}` (or legacy `#/definitions/Name`) -- a
///   reference to a named, hoisted schema (e.g. any `Option<Secret>` field,
///   since `Secret`'s `JsonSchema` impl gives it a name and schemars hoists
///   named schemas rather than inlining them).
/// - `{"anyOf": [<schema>, {"type": "null"}]}` -- how schemars renders
///   `Option<T>` once `T` is itself a `$ref` (a plain merged nullable type
///   isn't possible across a reference boundary). Resolved to the single
///   non-null variant.
///
/// Both can nest (an `anyOf`'s surviving variant is often itself a `$ref`),
/// so resolution recurses. Returns `node` unchanged if neither pattern
/// matches or the reference can't be resolved.
fn resolve_ref<'a>(root: &'a Value, node: &'a Value) -> &'a Value {
    if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
        let name = reference.rsplit('/').next().unwrap_or(reference);
        let target = root
            .get("$defs")
            .or_else(|| root.get("definitions"))
            .and_then(|defs| defs.get(name));
        return match target {
            Some(target) => resolve_ref(root, target),
            None => node,
        };
    }

    if let Some(variants) = node.get("anyOf").and_then(Value::as_array) {
        let mut non_null = variants
            .iter()
            .filter(|v| v.get("type").and_then(Value::as_str) != Some("null"));
        if let (Some(only), None) = (non_null.next(), non_null.next()) {
            return resolve_ref(root, only);
        }
    }

    node
}

/// A type-appropriate empty value, used to seed a fresh array item when no
/// default item remains to prefill it with.
fn zero_value_for(schema: &Value) -> Value {
    match get_promptable_type(schema).as_deref() {
        Some("object") => Value::Object(serde_json::Map::new()),
        Some("array") => Value::Array(vec![]),
        Some("string") => Value::String(String::new()),
        Some("integer") | Some("number") => Value::Number(0.into()),
        Some("boolean") => Value::Bool(false),
        _ => Value::Null,
    }
}

/*-- tests -----------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn secret_format_is_detected_from_schema_not_field_name() {
        assert!(is_secret_schema(
            &json!({"type": "string", "format": "password"})
        ));
        assert!(!is_secret_schema(&json!({"type": "string"})));
        // A field literally named "api_key" with no format marker is NOT a secret.
        assert!(!is_secret_schema(
            &json!({"type": "string", "title": "api_key"})
        ));
    }

    #[test]
    fn promptable_types_are_object_array_and_scalars() {
        assert_eq!(get_promptable_type(&json!({"type": "object"})), Some("object".to_string()));
        assert_eq!(get_promptable_type(&json!({"type": "array"})), Some("array".to_string()));
        assert_eq!(get_promptable_type(&json!({"type": "string"})), Some("string".to_string()));
        assert_eq!(get_promptable_type(&json!({"type": "integer"})), Some("integer".to_string()));
        assert_eq!(get_promptable_type(&json!({"type": "number"})), Some("number".to_string()));
        assert_eq!(get_promptable_type(&json!({"type": "boolean"})), Some("boolean".to_string()));
        assert_eq!(get_promptable_type(&json!({})), None);
        assert_eq!(get_promptable_type(&json!({"$ref": "#/$defs/Unresolved"})), None);
    }

    #[test]
    fn promptable_type_falls_back_to_any_of_for_option_types() {
        // When resolve_ref cannot fully unwrap anyOf (e.g. unresolved $ref
        // inside the non-null variant), get_promptable_type still extracts
        // the single-variant anyOf type.
        assert_eq!(
            get_promptable_type(&json!({"anyOf": [{"type": "string"}, {"type": "null"}]})),
            Some("string".to_string())
        );
        assert_eq!(
            get_promptable_type(&json!({"anyOf": [{"type": "integer"}, {"type": "null"}]})),
            Some("integer".to_string())
        );
        assert_eq!(
            get_promptable_type(&json!({"anyOf": [{"type": "boolean"}, {"type": "null"}]})),
            Some("boolean".to_string())
        );
        assert_eq!(
            get_promptable_type(&json!({"anyOf": [{"type": "object"}, {"type": "null"}]})),
            Some("object".to_string())
        );
        assert_eq!(
            get_promptable_type(&json!({"anyOf": [{"type": "array"}, {"type": "null"}]})),
            Some("array".to_string())
        );
        assert_eq!(
            get_promptable_type(&json!({"anyOf": [{"type": "number"}, {"type": "null"}]})),
            Some("number".to_string())
        );
        // Multiple non-null variants are NOT promptable.
        assert_eq!(
            get_promptable_type(&json!({"anyOf": [{"type": "string"}, {"type": "integer"}, {"type": "null"}]})),
            None
        );
        // anyOf without a type marker in variants is NOT promptable.
        assert_eq!(
            get_promptable_type(&json!({"anyOf": [{"$ref": "#/$defs/Unresolved"}, {"type": "null"}]})),
            None
        );
    }

    #[test]
    fn promptable_type_handles_type_as_array_for_option() {
        // Some schemars versions represent Option<T> as `"type": ["string", "null"]`
        // rather than using anyOf.
        assert_eq!(
            get_promptable_type(&json!({"type": ["string", "null"]})),
            Some("string".to_string())
        );
        assert_eq!(
            get_promptable_type(&json!({"type": ["integer", "null"]})),
            Some("integer".to_string())
        );
        assert_eq!(
            get_promptable_type(&json!({"type": ["boolean", "null"]})),
            Some("boolean".to_string())
        );
        assert_eq!(
            get_promptable_type(&json!({"type": ["number", "null"]})),
            Some("number".to_string())
        );
        assert_eq!(
            get_promptable_type(&json!({"type": ["array", "null"]})),
            Some("array".to_string())
        );
        assert_eq!(
            get_promptable_type(&json!({"type": ["object", "null"]})),
            Some("object".to_string())
        );
        // Null-only is NOT promptable.
        assert_eq!(get_promptable_type(&json!({"type": ["null"]})), None);
        // Multiple non-null types are NOT promptable.
        assert_eq!(
            get_promptable_type(&json!({"type": ["string", "integer", "null"]})),
            None
        );
    }

    #[test]
    fn ref_resolves_against_defs() {
        let root = json!({
            "$defs": {
                "Inner": {"type": "object", "properties": {"x": {"type": "integer"}}}
            }
        });
        let node = json!({"$ref": "#/$defs/Inner"});
        let resolved = resolve_ref(&root, &node);
        assert_eq!(resolved.get("type").and_then(Value::as_str), Some("object"));
    }

    #[test]
    fn ref_resolves_against_legacy_definitions() {
        let root = json!({
            "definitions": {
                "Inner": {"type": "string"}
            }
        });
        let node = json!({"$ref": "#/definitions/Inner"});
        let resolved = resolve_ref(&root, &node);
        assert_eq!(resolved.get("type").and_then(Value::as_str), Some("string"));
    }

    #[test]
    fn unresolvable_ref_falls_back_to_node_itself() {
        let root = json!({});
        let node = json!({"$ref": "#/$defs/Missing"});
        let resolved = resolve_ref(&root, &node);
        assert_eq!(resolved, &node);
    }

    #[test]
    fn any_of_option_wrapper_resolves_to_the_non_null_variant() {
        // How schemars renders a plain `Option<T>` where `T` isn't a `$ref`.
        let root = json!({});
        let node = json!({"anyOf": [{"type": "string"}, {"type": "null"}]});
        let resolved = resolve_ref(&root, &node);
        assert_eq!(resolved.get("type").and_then(Value::as_str), Some("string"));
    }

    #[test]
    fn any_of_option_wrapper_around_a_ref_resolves_through_both() {
        // How schemars renders `Option<Secret>`: the named `Secret` schema is hoisted
        // into `$defs` and referenced, and `Option<...>` wraps that ref in `anyOf`
        // alongside a null variant, since it can't merge "null" into a `$ref` inline.
        let root = json!({
            "$defs": {
                "Secret": {"type": "string", "format": "password"}
            }
        });
        let node = json!({"anyOf": [{"$ref": "#/$defs/Secret"}, {"type": "null"}]});
        let resolved = resolve_ref(&root, &node);
        assert_eq!(resolved.get("type").and_then(Value::as_str), Some("string"));
        assert!(is_secret_schema(resolved));
        assert_eq!(get_promptable_type(resolved), Some("string".to_string()));
    }

    #[test]
    fn zero_value_matches_schema_type() {
        assert_eq!(zero_value_for(&json!({"type": "string"})), json!(""));
        assert_eq!(zero_value_for(&json!({"type": "integer"})), json!(0));
        assert_eq!(zero_value_for(&json!({"type": "boolean"})), json!(false));
        assert_eq!(zero_value_for(&json!({"type": "array"})), json!([]));
        assert_eq!(zero_value_for(&json!({"type": "object"})), json!({}));
        // Also works with array-style type (Option<T>).
        assert_eq!(zero_value_for(&json!({"type": ["string", "null"]})), json!(""));
        assert_eq!(zero_value_for(&json!({"type": ["integer", "null"]})), json!(0));
    }
}
