use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use serde_json::{Map, Value};

const RUST_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
];

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut check_mode = false;
    for arg in &args {
        match arg.as_str() {
            "--check" => check_mode = true,
            "--help" | "-h" => {
                println!("Usage: ankic-codegen [--check]");
                println!("  Generates typed models and 21 namespace APIs from schemas/api-v1.schema.json.");
                println!("  --check  Verify generated files on disk match byte-for-byte without writing.");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("ankic-codegen: unknown argument `{other}`");
                return ExitCode::from(2);
            }
        }
    }

    let root = workspace_root();
    let schema_path = root.join("schemas/api-v1.schema.json");
    let schema_text = match fs::read_to_string(&schema_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ankic-codegen: failed to read {}: {e}", schema_path.display());
            return ExitCode::from(1);
        }
    };
    let schema: Value = match serde_json::from_str(&schema_text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ankic-codegen: failed to parse schema JSON: {e}");
            return ExitCode::from(1);
        }
    };

    let defs = schema
        .get("$defs")
        .and_then(Value::as_object)
        .expect("schema missing $defs object");
    let methods = schema
        .get("x-methods")
        .and_then(Value::as_object)
        .expect("schema missing x-methods object");

    let types_raw = generate_types(defs, methods);
    let async_api_raw = generate_api(defs, methods, true);
    let blocking_api_raw = generate_api(defs, methods, false);

    let targets: Vec<(PathBuf, String)> = vec![
        (
            root.join("crates/anki-connect-client/src/types/generated.rs"),
            format_rust(&types_raw),
        ),
        (
            root.join("crates/anki-connect-client/src/api/generated_async.rs"),
            format_rust(&async_api_raw),
        ),
        (
            root.join("crates/anki-connect-client/src/api/generated_blocking.rs"),
            format_rust(&blocking_api_raw),
        ),
    ];

    if check_mode {
        let mut drifted = false;
        for (path, expected) in &targets {
            match fs::read_to_string(path) {
                Ok(actual) if actual == *expected => {}
                Ok(actual) => {
                    drifted = true;
                    eprintln!("DRIFT detected in {}", path.display());
                    report_first_diff(&actual, expected);
                }
                Err(e) => {
                    drifted = true;
                    eprintln!("MISSING or unreadable {}: {e}", path.display());
                }
            }
        }
        if drifted {
            eprintln!("Run `cargo run -p ankic-codegen` to regenerate.");
            return ExitCode::from(1);
        }
        println!("ankic-codegen: all {} generated files are up to date.", targets.len());
        return ExitCode::SUCCESS;
    }

    for (path, content) in &targets {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("failed to create parent directory");
        }
        fs::write(path, content).expect("failed to write generated file");
        println!("Generated {}", path.display());
    }

    ExitCode::SUCCESS
}

fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest_dir
        .parent()
        .and_then(Path::parent)
        .unwrap_or(manifest_dir);
    if candidate.join("schemas/api-v1.schema.json").exists() {
        candidate.to_path_buf()
    } else {
        env::current_dir().expect("failed to determine current directory")
    }
}

fn format_rust(source: &str) -> String {
    let mut child = match Command::new("rustfmt")
        .args(["--edition", "2021"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return source.to_string(),
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(source.as_bytes());
    }
    match child.wait_with_output() {
        Ok(out) if out.status.success() => {
            String::from_utf8(out.stdout).unwrap_or_else(|_| source.to_string())
        }
        Ok(out) => {
            eprintln!(
                "ankic-codegen: rustfmt failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            source.to_string()
        }
        Err(_) => source.to_string(),
    }
}

fn report_first_diff(actual: &str, expected: &str) {
    let a_lines: Vec<&str> = actual.lines().collect();
    let e_lines: Vec<&str> = expected.lines().collect();
    let max = a_lines.len().max(e_lines.len());
    for i in 0..max {
        let a = a_lines.get(i).copied();
        let e = e_lines.get(i).copied();
        if a != e {
            eprintln!("  first difference at line {}:", i + 1);
            eprintln!("    actual  : {:?}", a);
            eprintln!("    expected: {:?}", e);
            return;
        }
    }
    eprintln!(
        "  file lengths differ (actual {} bytes, expected {} bytes)",
        actual.len(),
        expected.len()
    );
}

// ---------------------------------------------------------------------------
// Reachability & Schema Helpers
// ---------------------------------------------------------------------------

fn collect_refs(val: &Value, out: &mut BTreeSet<String>) {
    match val {
        Value::Object(map) => {
            if let Some(ref_str) = map.get("$ref").and_then(Value::as_str) {
                if let Some(name) = ref_str.strip_prefix("#/$defs/") {
                    out.insert(name.to_string());
                }
            }
            for v in map.values() {
                collect_refs(v, out);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                collect_refs(v, out);
            }
        }
        _ => {}
    }
}

fn collect_reachable_defs(
    defs: &Map<String, Value>,
    methods: &Map<String, Value>,
) -> BTreeSet<String> {
    let mut queue: BTreeSet<String> = BTreeSet::new();
    queue.insert("Operation".to_string());
    queue.insert("OperationHandle".to_string());
    queue.insert("OperationBase".to_string());

    for m in methods.values() {
        if let Some(m_obj) = m.as_object() {
            for key in ["params", "result", "operationResult"] {
                if let Some(v) = m_obj.get(key) {
                    collect_refs(v, &mut queue);
                }
            }
        }
    }

    let mut reachable = BTreeSet::new();
    while let Some(name) = queue.pop_first() {
        if !reachable.insert(name.clone()) {
            continue;
        }
        if let Some(def) = defs.get(&name) {
            collect_refs(def, &mut queue);
        }
    }
    reachable
}

fn ref_name(val: &Value) -> Option<&str> {
    val.get("$ref")
        .and_then(Value::as_str)
        .and_then(|s| s.strip_prefix("#/$defs/"))
}

fn is_null_schema(val: &Value) -> bool {
    val.get("type").and_then(Value::as_str) == Some("null")
}

/// If `val` is `{"anyOf": [T, {"type": "null"}]}` (in either order), returns `Some(T)`.
fn unwrap_nullable(val: &Value) -> Option<&Value> {
    let arr = val.get("anyOf").and_then(Value::as_array)?;
    if arr.len() != 2 {
        return None;
    }
    if is_null_schema(&arr[0]) && !is_null_schema(&arr[1]) {
        Some(&arr[1])
    } else if is_null_schema(&arr[1]) && !is_null_schema(&arr[0]) {
        Some(&arr[0])
    } else {
        None
    }
}

fn required_set(obj: &Map<String, Value>) -> BTreeSet<String> {
    obj.get("required")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Naming & Doc Comment Utilities
// ---------------------------------------------------------------------------

fn to_pascal_case(input: &str) -> String {
    let mut out = String::new();
    for part in input.split(|c: char| !c.is_ascii_alphanumeric()) {
        if part.is_empty() {
            continue;
        }
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

fn to_snake_case(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    for i in 0..chars.len() {
        let c = chars[i];
        if !c.is_ascii_alphanumeric() {
            if !out.ends_with('_') && !out.is_empty() {
                out.push('_');
            }
            continue;
        }
        if c.is_ascii_uppercase() {
            let prev_lower_or_digit = i > 0 && (chars[i - 1].is_ascii_lowercase() || chars[i - 1].is_ascii_digit());
            let next_lower = i + 1 < chars.len() && chars[i + 1].is_ascii_lowercase();
            let prev_upper = i > 0 && chars[i - 1].is_ascii_uppercase();
            if !out.is_empty() && !out.ends_with('_') && (prev_lower_or_digit || (prev_upper && next_lower)) {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn snake_to_camel(snake: &str) -> String {
    let mut out = String::new();
    let mut capitalize = false;
    for c in snake.chars() {
        if c == '_' {
            capitalize = true;
        } else if capitalize {
            out.push(c.to_ascii_uppercase());
            capitalize = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn escape_ident(ident: &str) -> String {
    if RUST_KEYWORDS.contains(&ident) {
        format!("r#{ident}")
    } else {
        ident.to_string()
    }
}

fn singularize(name: &str) -> &str {
    if let Some(stripped) = name.strip_suffix('s') {
        if !stripped.ends_with('s') && stripped.len() >= 2 {
            return stripped;
        }
    }
    name
}

fn sanitize_doc_line(line: &str) -> String {
    // Escape square brackets outside backticks to avoid rustdoc::broken_intra_doc_links.
    let mut out = String::with_capacity(line.len());
    let mut in_code = false;
    for ch in line.chars() {
        if ch == '`' {
            in_code = !in_code;
            out.push(ch);
        } else if !in_code && (ch == '[' || ch == ']') {
            out.push('\\');
            out.push(ch);
        } else {
            out.push(ch);
        }
    }
    out
}

fn emit_doc(buf: &mut String, indent: &str, text: &str) {
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            buf.push_str(indent);
            buf.push_str("///\n");
        } else {
            buf.push_str(indent);
            buf.push_str("/// ");
            buf.push_str(&sanitize_doc_line(trimmed));
            buf.push('\n');
        }
    }
}

// ---------------------------------------------------------------------------
// Type Generator
// ---------------------------------------------------------------------------

struct TypeGen<'a> {
    defs: &'a Map<String, Value>,
    op_result_defs: BTreeSet<String>,
    /// Ordered map of emitted Rust type name -> source code block.
    emitted: BTreeMap<String, String>,
}

impl<'a> TypeGen<'a> {
    fn new(defs: &'a Map<String, Value>, methods: &Map<String, Value>) -> Self {
        let mut op_result_defs = BTreeSet::new();
        for m in methods.values() {
            if m.get("completion").and_then(Value::as_str) == Some("operation") {
                if let Some(rname) = m.get("result").and_then(ref_name) {
                    op_result_defs.insert(rname.to_string());
                }
            }
        }
        Self {
            defs,
            op_result_defs,
            emitted: BTreeMap::new(),
        }
    }

    fn register(&mut self, name: String, code: String) {
        if let Some(existing) = self.emitted.get(&name) {
            if existing != &code {
                panic!("Duplicate conflicting type name generated: {name}");
            }
            return;
        }
        self.emitted.insert(name, code);
    }

    fn emit_top_level(&mut self, def_name: &str) {
        let rust_name = to_pascal_case(def_name);
        let def = self
            .defs
            .get(def_name)
            .unwrap_or_else(|| panic!("Missing $def `{def_name}`"));
        let desc = def.get("description").and_then(Value::as_str);

        // 1. Operation method *_Result -> alias to OperationHandle.
        if self.op_result_defs.contains(def_name) {
            let mut buf = String::new();
            if let Some(d) = desc {
                emit_doc(&mut buf, "", d);
            } else {
                emit_doc(&mut buf, "", &format!("Operation handle returned by `{def_name}`."));
            }
            buf.push_str(&format!("pub type {rust_name} = OperationHandle;\n"));
            self.register(rust_name, buf);
            return;
        }

        // 2. Special handling for Operation (60-branch oneOf of OperationBase + method/state/result/error).
        if def_name == "Operation" {
            self.emit_operation();
            return;
        }

        // 3. Direct $ref alias.
        if let Some(target) = ref_name(def) {
            let target_rust = to_pascal_case(target);
            let mut buf = String::new();
            let fallback = format!("Type alias for [`{target_rust}`] (`{def_name}`).");
            emit_doc(&mut buf, "", desc.unwrap_or(&fallback));
            buf.push_str(&format!("pub type {rust_name} = {target_rust};\n"));
            self.register(rust_name, buf);
            return;
        }

        // 4. Top-level nullable anyOf: [T, null].
        if let Some(inner) = unwrap_nullable(def) {
            let inner_ty = self.resolve_type(&rust_name, inner);
            let mut buf = String::new();
            let fallback = format!("Nullable `{def_name}` value.");
            emit_doc(&mut buf, "", desc.unwrap_or(&fallback));
            buf.push_str(&format!("pub type {rust_name} = Option<{inner_ty}>;\n"));
            self.register(rust_name, buf);
            return;
        }

        // 5. Top-level string enum.
        if let Some(variants) = def.get("enum").and_then(Value::as_array) {
            self.emit_unit_enum(&rust_name, desc, variants);
            return;
        }

        // 6. Top-level oneOf (tagged or untagged enum).
        if let Some(branches) = def.get("oneOf").and_then(Value::as_array) {
            if def.get("properties").is_none() {
                self.emit_one_of_enum(&rust_name, desc, branches);
                return;
            }
        }

        // 7. Top-level array.
        if def.get("type").and_then(Value::as_str) == Some("array") {
            let item_schema = def.get("items").expect("array missing items");
            let item_ty = self.resolve_type(&format!("{rust_name}Item"), item_schema);
            let mut buf = String::new();
            let fallback = format!("List of `{def_name}` items.");
            emit_doc(&mut buf, "", desc.unwrap_or(&fallback));
            buf.push_str(&format!("pub type {rust_name} = Vec<{item_ty}>;\n"));
            self.register(rust_name, buf);
            return;
        }

        // 8. Top-level string scalar.
        if def.get("type").and_then(Value::as_str) == Some("string") {
            let mut buf = String::new();
            let fallback = format!("String value for `{def_name}`.");
            emit_doc(&mut buf, "", desc.unwrap_or(&fallback));
            buf.push_str(&format!("pub type {rust_name} = String;\n"));
            self.register(rust_name, buf);
            return;
        }

        // 9. Top-level object.
        if def.get("type").and_then(Value::as_str) == Some("object") {
            let obj = def.as_object().unwrap();
            if let Some(props) = obj.get("properties").and_then(Value::as_object) {
                let req = required_set(obj);
                self.emit_struct(&rust_name, desc, props, &req);
                return;
            }
            if let Some(addl) = obj.get("additionalProperties").and_then(Value::as_object) {
                let val_ty = self.resolve_type(&format!("{rust_name}Value"), &Value::Object(addl.clone()));
                let mut buf = String::new();
                let fallback = format!("Map of `{def_name}` entries.");
                emit_doc(&mut buf, "", desc.unwrap_or(&fallback));
                buf.push_str(&format!("pub type {rust_name} = BTreeMap<String, {val_ty}>;\n"));
                self.register(rust_name, buf);
                return;
            }
        }

        panic!("Unsupported top-level $def `{def_name}`: {def}");
    }

    fn emit_operation(&mut self) {
        let state_enum = r#"/// Lifecycle state of an asynchronous operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OperationState {
    /// Operation is queued and waiting to start.
    #[serde(rename = "queued")]
    Queued,
    /// Operation is currently executing.
    #[serde(rename = "running")]
    Running,
    /// Operation is waiting for user interaction in Anki.
    #[serde(rename = "waitingForUser")]
    WaitingForUser,
    /// Operation completed successfully.
    #[serde(rename = "succeeded")]
    Succeeded,
    /// Operation terminated with an error.
    #[serde(rename = "failed")]
    Failed,
    /// Operation was cancelled before completion.
    #[serde(rename = "cancelled")]
    Cancelled,
}
"#;
        self.register("OperationState".to_string(), state_enum.to_string());

        let op_struct = r#"/// Status and result of an asynchronous operation returned by `operations.get` or `operations.cancel`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    /// Unique operation identifier.
    pub operation_id: String,
    /// RPC method name that initiated the operation.
    pub method: String,
    /// Associated collection session token, if collection-scoped.
    pub session: Option<SessionId>,
    /// Timestamp when the operation was created.
    pub created_at: Timestamp,
    /// Timestamp when the operation was last updated.
    pub updated_at: Timestamp,
    /// Whether the operation can be cancelled via `operations.cancel`.
    pub cancellable: bool,
    /// Optional progress information while the operation is running.
    pub progress: Option<Progress>,
    /// Current lifecycle state of the operation.
    pub state: OperationState,
    /// Final result payload when `state` is [`OperationState::Succeeded`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Structured error payload when `state` is [`OperationState::Failed`] or [`OperationState::Cancelled`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Error>,
}
"#;
        self.register("Operation".to_string(), op_struct.to_string());
    }

    fn emit_unit_enum(&mut self, rust_name: &str, desc: Option<&str>, variants: &[Value]) {
        let mut buf = String::new();
        let fallback = format!("Enumerated values for `{rust_name}`.");
        emit_doc(&mut buf, "", desc.unwrap_or(&fallback));
        buf.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]\n");
        buf.push_str(&format!("pub enum {rust_name} {{\n"));
        for v in variants {
            let s = v
                .as_str()
                .unwrap_or_else(|| panic!("Non-string enum variant in {rust_name}: {v}"));
            let var_name = to_pascal_case(s);
            emit_doc(&mut buf, "    ", &format!("JSON value `{s}`."));
            buf.push_str(&format!("    #[serde(rename = \"{s}\")]\n"));
            buf.push_str(&format!("    {var_name},\n"));
        }
        buf.push_str("}\n");
        self.register(rust_name.to_string(), buf);
    }

    fn emit_struct(
        &mut self,
        rust_name: &str,
        desc: Option<&str>,
        props: &Map<String, Value>,
        required: &BTreeSet<String>,
    ) {
        // Sort properties deterministically (BTreeMap).
        let sorted_props: BTreeMap<&String, &Value> = props.iter().collect();

        let mut fields_code = String::new();
        let mut all_defaultable = true;

        for (json_key, prop_schema) in sorted_props {
            let is_required = required.contains(json_key.as_str());
            let (ty, is_option) = self.resolve_field_type(rust_name, json_key, prop_schema, is_required);
            if !is_option {
                all_defaultable = false;
            }

            let fallback_field_doc = format!("JSON field `{json_key}`.");
            let pdesc = prop_schema
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or(&fallback_field_doc);
            emit_doc(&mut fields_code, "    ", pdesc);

            let snake = to_snake_case(json_key);
            let field_ident = escape_ident(&snake);
            let needs_explicit_rename =
                field_ident.starts_with("r#") || snake_to_camel(&snake) != *json_key;

            let mut serde_attrs: Vec<String> = Vec::new();
            if needs_explicit_rename {
                serde_attrs.push(format!("rename = \"{json_key}\""));
            }
            if !is_required {
                serde_attrs.push("default".to_string());
                serde_attrs.push("skip_serializing_if = \"Option::is_none\"".to_string());
            } else if is_option {
                // Required nullable field: must serialize `null` when `None`,
                // but `#[serde(default)]` lets deserialization remain lenient.
                serde_attrs.push("default".to_string());
            }

            if !serde_attrs.is_empty() {
                fields_code.push_str(&format!("    #[serde({})]\n", serde_attrs.join(", ")));
            }
            fields_code.push_str(&format!("    pub {field_ident}: {ty},\n"));
        }

        let mut buf = String::new();
        let fallback_struct_doc = format!("Schema type `{rust_name}`.");
        emit_doc(&mut buf, "", desc.unwrap_or(&fallback_struct_doc));
        if all_defaultable {
            buf.push_str("#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]\n");
        } else {
            buf.push_str("#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\n");
        }
        if !props.is_empty() {
            buf.push_str("#[serde(rename_all = \"camelCase\")]\n");
        }
        buf.push_str(&format!("pub struct {rust_name} {{\n"));
        buf.push_str(&fields_code);
        buf.push_str("}\n");

        self.register(rust_name.to_string(), buf);
    }

    fn emit_one_of_enum(&mut self, rust_name: &str, desc: Option<&str>, branches: &[Value]) {
        // Check if all branches are objects sharing a single `const` string discriminator field.
        if let Some(tag_field) = detect_const_tag(branches) {
            self.emit_tagged_enum(rust_name, desc, &tag_field, branches);
            return;
        }
        self.emit_untagged_enum(rust_name, desc, branches);
    }

    fn emit_tagged_enum(
        &mut self,
        rust_name: &str,
        desc: Option<&str>,
        tag_field: &str,
        branches: &[Value],
    ) {
        let mut buf = String::new();
        let fallback = format!("Tagged union for `{rust_name}`.");
        emit_doc(&mut buf, "", desc.unwrap_or(&fallback));
        buf.push_str("#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\n");
        buf.push_str(&format!("#[serde(tag = \"{tag_field}\")]\n"));
        buf.push_str(&format!("pub enum {rust_name} {{\n"));

        for branch in branches {
            let obj = branch.as_object().expect("tagged branch must be object");
            let props = obj.get("properties").and_then(Value::as_object).unwrap();
            let req = required_set(obj);
            let tag_val = props[tag_field]
                .get("const")
                .and_then(Value::as_str)
                .unwrap();
            let variant_name = to_pascal_case(tag_val);

            let fallback_var_doc = format!("Variant tagged with `{tag_field} = \"{tag_val}\"`.");
            let vdesc = obj
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or(&fallback_var_doc);
            emit_doc(&mut buf, "    ", vdesc);
            buf.push_str(&format!("    #[serde(rename = \"{tag_val}\")]\n"));
            let sorted_props: BTreeMap<&String, &Value> = props
                .iter()
                .filter(|(k, _)| k.as_str() != tag_field)
                .collect();

            if sorted_props.is_empty() {
                buf.push_str(&format!("    {variant_name},\n"));
            } else {
                buf.push_str(&format!("    {variant_name} {{\n"));
                for (json_key, prop_schema) in sorted_props {
                    let is_required = req.contains(json_key.as_str());
                    let (ty, is_option) = self.resolve_field_type(
                        &format!("{rust_name}{variant_name}"),
                        json_key,
                        prop_schema,
                        is_required,
                    );
                    let fallback_field_doc = format!("JSON field `{json_key}`.");
                    let pdesc = prop_schema
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or(&fallback_field_doc);
                    emit_doc(&mut buf, "        ", pdesc);

                    let snake = to_snake_case(json_key);
                    let field_ident = escape_ident(&snake);
                    let mut serde_attrs = Vec::new();
                    if field_ident != *json_key {
                        serde_attrs.push(format!("rename = \"{json_key}\""));
                    }
                    if !is_required {
                        serde_attrs.push("default".to_string());
                        serde_attrs.push("skip_serializing_if = \"Option::is_none\"".to_string());
                    } else if is_option {
                        serde_attrs.push("default".to_string());
                    }
                    if !serde_attrs.is_empty() {
                        buf.push_str(&format!("        #[serde({})]\n", serde_attrs.join(", ")));
                    }
                    buf.push_str(&format!("        {field_ident}: {ty},\n"));
                }
                buf.push_str("    },\n");
            }
        }

        buf.push_str("}\n");
        self.register(rust_name.to_string(), buf);
    }

    fn emit_untagged_enum(&mut self, rust_name: &str, desc: Option<&str>, branches: &[Value]) {
        let mut buf = String::new();
        let fallback = format!("Untagged union for `{rust_name}`.");
        emit_doc(&mut buf, "", desc.unwrap_or(&fallback));
        buf.push_str("#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\n");
        buf.push_str("#[serde(untagged)]\n");
        buf.push_str(&format!("pub enum {rust_name} {{\n"));

        // Determine if this is a 2-branch (result / error) batch item enum.
        let is_result_error_pair = branches.len() == 2
            && branches[0]
                .get("properties")
                .and_then(|p| p.get("result"))
                .is_some()
            && branches[1]
                .get("properties")
                .and_then(|p| p.get("error"))
                .is_some();

        for (idx, branch) in branches.iter().enumerate() {
            let obj = branch.as_object().expect("untagged branch must be object");
            let props = obj.get("properties").and_then(Value::as_object).unwrap();
            let req = required_set(obj);

            let variant_name = if is_result_error_pair {
                if idx == 0 {
                    "Success".to_string()
                } else {
                    "Error".to_string()
                }
            } else if props.len() == 1 {
                let only_key = props.keys().next().unwrap();
                to_pascal_case(only_key)
            } else {
                format!("Variant{}", idx + 1)
            };

            let fallback_var_doc = format!("Variant `{variant_name}`.");
            let vdesc = obj
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or(&fallback_var_doc);
            emit_doc(&mut buf, "    ", vdesc);

            let sorted_props: BTreeMap<&String, &Value> = props.iter().collect();
            buf.push_str(&format!("    {variant_name} {{\n"));
            for (json_key, prop_schema) in sorted_props {
                let is_required = req.contains(json_key.as_str());
                let (mut ty, is_option) = self.resolve_field_type(
                    &format!("{rust_name}{variant_name}"),
                    json_key,
                    prop_schema,
                    is_required,
                );
                // Box large domain structs inside batch item enums to satisfy clippy::large_enum_variant.
                if is_result_error_pair && json_key == "result" && matches!(ty.as_str(), "Card" | "Note") {
                    ty = format!("Box<{ty}>");
                }
                let fallback_field_doc = format!("JSON field `{json_key}`.");
                let pdesc = prop_schema
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or(&fallback_field_doc);
                emit_doc(&mut buf, "        ", pdesc);

                let snake = to_snake_case(json_key);
                let field_ident = escape_ident(&snake);
                let mut serde_attrs = Vec::new();
                if field_ident != *json_key {
                    serde_attrs.push(format!("rename = \"{json_key}\""));
                }
                if !is_required {
                    serde_attrs.push("default".to_string());
                    serde_attrs.push("skip_serializing_if = \"Option::is_none\"".to_string());
                } else if is_option {
                    serde_attrs.push("default".to_string());
                }
                if !serde_attrs.is_empty() {
                    buf.push_str(&format!("        #[serde({})]\n", serde_attrs.join(", ")));
                }
                buf.push_str(&format!("        {field_ident}: {ty},\n"));
            }
            buf.push_str("    },\n");
        }

        buf.push_str("}\n");
        self.register(rust_name.to_string(), buf);
    }

    /// Resolves a struct/variant field's Rust type and whether it is `Option<...>`.
    fn resolve_field_type(
        &mut self,
        parent_name: &str,
        json_key: &str,
        schema: &Value,
        is_required: bool,
    ) -> (String, bool) {
        let hint = format!("{parent_name}{}", to_pascal_case(json_key));

        if is_null_schema(schema) {
            return ("Option<String>".to_string(), true);
        }

        if let Some(inner) = unwrap_nullable(schema) {
            let inner_ty = self.resolve_type(&hint, inner);
            return (format!("Option<{inner_ty}>"), true);
        }

        let base_ty = self.resolve_type(&hint, schema);
        if is_required {
            (base_ty, false)
        } else {
            (format!("Option<{base_ty}>"), true)
        }
    }

    fn resolve_type(&mut self, name_hint: &str, schema: &Value) -> String {
        let Some(obj) = schema.as_object() else {
            return "serde_json::Value".to_string();
        };

        if obj.is_empty() {
            return "serde_json::Value".to_string();
        }

        if let Some(r) = ref_name(schema) {
            return to_pascal_case(r);
        }

        if let Some(c) = obj.get("const") {
            return match c {
                Value::String(_) => "String".to_string(),
                Value::Bool(_) => "bool".to_string(),
                Value::Number(n) => {
                    if n.is_u64() {
                        "u64".to_string()
                    } else if n.is_i64() {
                        "i64".to_string()
                    } else {
                        "f64".to_string()
                    }
                }
                _ => "serde_json::Value".to_string(),
            };
        }

        if let Some(variants) = obj.get("enum").and_then(Value::as_array) {
            let desc = obj.get("description").and_then(Value::as_str);
            self.emit_unit_enum(name_hint, desc, variants);
            return name_hint.to_string();
        }

        if let Some(inner) = unwrap_nullable(schema) {
            let inner_ty = self.resolve_type(name_hint, inner);
            return format!("Option<{inner_ty}>");
        }

        match obj.get("type").and_then(Value::as_str) {
            Some("string") => "String".to_string(),
            Some("boolean") => "bool".to_string(),
            Some("integer") => {
                let min_non_neg = obj
                    .get("minimum")
                    .and_then(Value::as_i64)
                    .is_some_and(|m| m >= 0);
                if min_non_neg {
                    "u64".to_string()
                } else {
                    "i64".to_string()
                }
            }
            Some("number") => "f64".to_string(),
            Some("null") => "Option<String>".to_string(),
            Some("array") => {
                let item_schema = obj.get("items").expect("array missing items");
                let item_hint = singularize(name_hint).to_string();
                let item_ty = self.resolve_type(&item_hint, item_schema);
                format!("Vec<{item_ty}>")
            }
            Some("object") => {
                if let Some(props) = obj.get("properties").and_then(Value::as_object) {
                    let req = required_set(obj);
                    let desc = obj.get("description").and_then(Value::as_str);
                    self.emit_struct(name_hint, desc, props, &req);
                    name_hint.to_string()
                } else if let Some(addl) = obj.get("additionalProperties").and_then(Value::as_object)
                {
                    let val_ty = self.resolve_type(
                        &format!("{name_hint}Value"),
                        &Value::Object(addl.clone()),
                    );
                    format!("BTreeMap<String, {val_ty}>")
                } else {
                    "BTreeMap<String, serde_json::Value>".to_string()
                }
            }
            _ => "serde_json::Value".to_string(),
        }
    }
}

fn detect_const_tag(branches: &[Value]) -> Option<String> {
    let first_props = branches.first()?.get("properties")?.as_object()?;
    for (key, val) in first_props {
        if val.get("const").and_then(Value::as_str).is_some() {
            let all_match = branches.iter().all(|b| {
                b.get("properties")
                    .and_then(|p| p.get(key))
                    .and_then(|v| v.get("const"))
                    .and_then(Value::as_str)
                    .is_some()
            });
            if all_match {
                return Some(key.clone());
            }
        }
    }
    None
}

fn generate_types(defs: &Map<String, Value>, methods: &Map<String, Value>) -> String {
    let reachable = collect_reachable_defs(defs, methods);
    let mut gen = TypeGen::new(defs, methods);
    for def_name in &reachable {
        gen.emit_top_level(def_name);
    }

    let mut out = String::new();
    out.push_str("// @generated by `ankic-codegen` from `schemas/api-v1.schema.json`. DO NOT EDIT.\n\n");
    out.push_str("use std::collections::BTreeMap;\n\n");
    out.push_str("use serde::{Deserialize, Serialize};\n\n");

    for code in gen.emitted.values() {
        out.push_str(code);
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------------------
// Namespace API Generator (Async & Blocking)
// ---------------------------------------------------------------------------

struct MethodSpec {
    rpc_name: String,
    rust_name: String,
    summary: String,
    params_type: String,
    result_type: String,
    zero_param: bool,
    op_result_type: Option<String>,
}

fn collect_namespaces(
    defs: &Map<String, Value>,
    methods: &Map<String, Value>,
) -> BTreeMap<String, Vec<MethodSpec>> {
    let mut by_ns: BTreeMap<String, Vec<MethodSpec>> = BTreeMap::new();

    for (rpc_name, m_val) in methods {
        let m = m_val.as_object().expect("method entry must be object");
        let (ns, rest) = rpc_name
            .split_once('.')
            .unwrap_or_else(|| panic!("invalid method name `{rpc_name}`"));

        let params_def = m.get("params").and_then(ref_name).expect("method missing params $ref");
        let result_def = m.get("result").and_then(ref_name).expect("method missing result $ref");
        let is_op = m.get("completion").and_then(Value::as_str) == Some("operation");
        let op_result_type = if is_op {
            let op_def = m
                .get("operationResult")
                .and_then(ref_name)
                .expect("operation method missing operationResult $ref");
            Some(to_pascal_case(op_def))
        } else {
            None
        };

        let zero_param = defs
            .get(params_def)
            .and_then(|d| d.get("properties"))
            .and_then(Value::as_object)
            .is_some_and(|props| props.keys().all(|k| k == "session"));

        let summary = m
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let rust_name = escape_ident(&to_snake_case(rest));
        let result_type = if is_op {
            "OperationHandle".to_string()
        } else {
            to_pascal_case(result_def)
        };

        by_ns.entry(ns.to_string()).or_default().push(MethodSpec {
            rpc_name: rpc_name.clone(),
            rust_name,
            summary,
            params_type: to_pascal_case(params_def),
            result_type,
            zero_param,
            op_result_type,
        });
    }

    for list in by_ns.values_mut() {
        list.sort_by(|a, b| a.rpc_name.cmp(&b.rpc_name));
    }
    by_ns
}

fn generate_api(defs: &Map<String, Value>, methods: &Map<String, Value>, is_async: bool) -> String {
    let by_ns = collect_namespaces(defs, methods);
    let client_import = if is_async {
        "use crate::client::AnkiClient;"
    } else {
        "use crate::client::blocking::AnkiClient;"
    };
    let async_kw = if is_async { "async " } else { "" };
    let await_sfx = if is_async { ".await" } else { "" };

    let mut out = String::new();
    out.push_str("// @generated by `ankic-codegen` from `schemas/api-v1.schema.json`. DO NOT EDIT.\n\n");
    if is_async {
        out.push_str("//! Generated asynchronous typed namespace APIs for Anki-Connect API v1.\n\n");
    } else {
        out.push_str("//! Generated blocking (synchronous) typed namespace APIs for Anki-Connect API v1.\n\n");
    }
    out.push_str(client_import);
    out.push('\n');
    out.push_str("use crate::error::Result;\n");
    out.push_str("use crate::types::*;\n\n");

    // Accessor methods on AnkiClient
    out.push_str("impl AnkiClient {\n");
    for ns in by_ns.keys() {
        let accessor = escape_ident(&to_snake_case(ns));
        let api_struct = format!("{}Api", to_pascal_case(ns));
        emit_doc(
            &mut out,
            "    ",
            &format!("Access the `{ns}` namespace API."),
        );
        out.push_str(&format!(
            "    pub fn {accessor}(&self) -> {api_struct}<'_> {{\n        {api_struct} {{ client: self }}\n    }}\n\n"
        ));
    }
    out.push_str("}\n\n");

    // Namespace sub-client structs and methods
    for (ns, specs) in &by_ns {
        let api_struct = format!("{}Api", to_pascal_case(ns));
        emit_doc(
            &mut out,
            "",
            &format!("Typed client for the `{ns}` namespace."),
        );
        out.push_str("#[derive(Debug, Clone, Copy)]\n");
        out.push_str(&format!("pub struct {api_struct}<'a> {{\n"));
        out.push_str("    client: &'a AnkiClient,\n");
        out.push_str("}\n\n");

        out.push_str(&format!("impl {api_struct}<'_> {{\n"));
        for spec in specs {
            let doc = if spec.summary.is_empty() {
                format!("Call `{}`.", spec.rpc_name)
            } else {
                format!("{}\n\nCalls RPC method `{}`.", spec.summary, spec.rpc_name)
            };
            emit_doc(&mut out, "    ", &doc);

            if spec.zero_param {
                out.push_str(&format!(
                    "    pub {async_kw}fn {}(&self) -> Result<{}> {{\n",
                    spec.rust_name, spec.result_type
                ));
                out.push_str(&format!(
                    "        self.client.call(\"{}\", &{}::default()){await_sfx}\n",
                    spec.rpc_name, spec.params_type
                ));
                out.push_str("    }\n\n");
            } else {
                out.push_str(&format!(
                    "    pub {async_kw}fn {}(&self, params: &{}) -> Result<{}> {{\n",
                    spec.rust_name, spec.params_type, spec.result_type
                ));
                out.push_str(&format!(
                    "        self.client.call(\"{}\", params){await_sfx}\n",
                    spec.rpc_name
                ));
                out.push_str("    }\n\n");
            }

            if let Some(op_result_ty) = &spec.op_result_type {
                let raw_name = spec.rust_name.strip_prefix("r#").unwrap_or(&spec.rust_name);
                let wait_name = format!("{raw_name}_and_wait");
                let wait_doc = format!(
                    "{}\n\nCalls RPC method `{}` and polls `operations.get` until completion (up to `timeout_secs` seconds).",
                    spec.summary, spec.rpc_name
                );
                emit_doc(&mut out, "    ", &wait_doc);

                if spec.zero_param {
                    out.push_str(&format!(
                        "    pub {async_kw}fn {wait_name}(&self, timeout_secs: u64) -> Result<{op_result_ty}> {{\n"
                    ));
                    out.push_str(&format!(
                        "        let handle = self.{}(){await_sfx}?;\n",
                        spec.rust_name
                    ));
                } else {
                    out.push_str(&format!(
                        "    pub {async_kw}fn {wait_name}(&self, params: &{}, timeout_secs: u64) -> Result<{op_result_ty}> {{\n",
                        spec.params_type
                    ));
                    out.push_str(&format!(
                        "        let handle = self.{}(params){await_sfx}?;\n",
                        spec.rust_name
                    ));
                }
                out.push_str(&format!(
                    "        self.client.wait_for_operation(&handle.operation_id, timeout_secs){await_sfx}\n"
                ));
                out.push_str("    }\n\n");
            }
        }
        out.push_str("}\n\n");
    }

    out
}
