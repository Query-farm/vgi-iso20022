//! Shared helpers for the per-object discovery/description metadata that the
//! `vgi-lint` strict profile expects on every function and table.
//!
//! Each function/table surfaces these in its `FunctionMetadata.tags`:
//! - `vgi.title` (VGI124)        — human-friendly display name
//! - `vgi.doc_llm` (VGI112)      — concise prose aimed at LLMs
//! - `vgi.doc_md` (VGI113)       — short Markdown description
//! - `vgi.keywords` (VGI126/138) — a JSON array of search terms/synonyms
//!
//! Table functions additionally declare their **result schema** structurally —
//! `vgi.result_columns_schema` (static) or `vgi.result_dynamic_columns_md`
//! (dynamic) — both **generated from the Arrow schema** so the metadata and the
//! bytes on the wire can never drift (VGI307/321/322/323/414/910). The retired
//! free-form `vgi.result_columns_md` is no longer emitted.
//!
//! Per-object `vgi.source_url` is intentionally NOT emitted here: it belongs on
//! the catalog object only (VGI139), which already carries the worker's
//! `source_url` (the project repository). The open ISO 20022 standard itself
//! (iso20022.org) is cited in the catalog `doc_md` prose.

use arrow_schema::{DataType, SchemaRef};

/// JSON-escape a string value (quotes, backslashes, control chars).
fn json_esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Encode comma-separated keywords as the JSON array of strings that
/// `vgi.keywords` requires (VGI138).
pub fn keywords_json(keywords: &str) -> String {
    let items: Vec<String> = keywords
        .split(',')
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(|k| format!("\"{}\"", json_esc(k)))
        .collect();
    format!("[{}]", items.join(","))
}

/// Render an Arrow [`DataType`] as the equivalent DuckDB type name, so a declared
/// `vgi.result_columns_schema` type (VGI322) matches what DuckDB reports for the
/// column the worker actually returns (VGI910).
pub fn duckdb_type(dt: &DataType) -> String {
    match dt {
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => "VARCHAR".to_string(),
        DataType::Boolean => "BOOLEAN".to_string(),
        DataType::Int32 => "INTEGER".to_string(),
        DataType::Int64 => "BIGINT".to_string(),
        DataType::Date32 => "DATE".to_string(),
        DataType::Decimal128(p, s) => format!("DECIMAL({p},{s})"),
        DataType::Timestamp(_, Some(_)) => "TIMESTAMP WITH TIME ZONE".to_string(),
        DataType::Timestamp(_, None) => "TIMESTAMP".to_string(),
        DataType::List(field) | DataType::LargeList(field) => {
            format!("{}[]", duckdb_type(field.data_type()))
        }
        DataType::Map(field, _) => {
            // The map entry is a struct of (key, value); render MAP(k, v).
            if let DataType::Struct(fields) = field.data_type() {
                if fields.len() == 2 {
                    return format!(
                        "MAP({}, {})",
                        duckdb_type(fields[0].data_type()),
                        duckdb_type(fields[1].data_type())
                    );
                }
            }
            "MAP(VARCHAR, VARCHAR)".to_string()
        }
        // Fallback: the timeunit-agnostic default (none of our columns hit this).
        other => format!("{other:?}"),
    }
}

/// The `comment` a `commented()` field carries (used as the column description).
fn field_comment(f: &arrow_schema::Field) -> String {
    f.metadata()
        .get("comment")
        .cloned()
        .unwrap_or_else(|| f.name().clone())
}

/// Build `vgi.result_columns_schema` — a JSON array of `{name, type, description}`
/// for a **static** table-function result — directly from its Arrow schema, so
/// the declared shape always matches the emitted batch (VGI307/321/322/323/910).
pub fn result_columns_schema_json(schema: &SchemaRef) -> String {
    let items: Vec<String> = schema
        .fields()
        .iter()
        .map(|f| {
            format!(
                "{{\"name\":\"{}\",\"type\":\"{}\",\"description\":\"{}\"}}",
                json_esc(f.name()),
                json_esc(&duckdb_type(f.data_type())),
                json_esc(&field_comment(f))
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}

/// Build `vgi.result_dynamic_columns_md` for a per-message exploder whose result
/// schema **varies by argument**: every column of the input relation is passed
/// through first (repeated once per child row), followed by the fixed child
/// columns. The child columns are rendered from the Arrow `child_schema` as a
/// `Name | Type | Description` table (VGI307/326/322/323); a leading `raw` row
/// documents the message passthrough column the examples supply (VGI910).
pub fn result_dynamic_columns_md(child_schema: &SchemaRef) -> String {
    let mut md = String::from(
        "The result schema is **dynamic**: every column of the input relation you pass is \
         returned first, unchanged and repeated once per child row, followed by the fixed child \
         columns below. When the input is the output of a `*_read` function (or any relation that \
         exposes a `raw` message column), that `raw` column is passed through too.\n\n\
         | Name | Type | Description |\n| --- | --- | --- |\n\
         | raw | VARCHAR | Passthrough: the parent message text carried from the input relation \
         (present whenever the input exposes a `raw` column). |\n",
    );
    for f in child_schema.fields() {
        md.push_str(&format!(
            "| {} | {} | {} |\n",
            f.name(),
            duckdb_type(f.data_type()),
            // Pipes would break the table; our comments contain none, but be safe.
            field_comment(f).replace('|', "\\|")
        ));
    }
    md
}

/// Build the four standard per-object discovery/description tags.
pub fn object_tags(
    title: &str,
    description_llm: &str,
    description_md: &str,
    keywords: &str,
) -> Vec<(String, String)> {
    vec![
        ("vgi.title".to_string(), title.to_string()),
        ("vgi.doc_llm".to_string(), description_llm.to_string()),
        ("vgi.doc_md".to_string(), description_md.to_string()),
        ("vgi.keywords".to_string(), keywords_json(keywords)),
    ]
}

/// One analyst task for the catalog-level `vgi.agent_test_tasks` suite that
/// `vgi-lint simulate` (VGI520 coverage / VGI920 agent-sim) runs. Only `prompt`
/// is ever shown to the analyst; `reference_sql` / `check_sql` /
/// `success_criteria` are grader-only.
pub struct AgentTask {
    pub name: String,
    pub prompt: String,
    /// Canonical solution used for deterministic result-compare grading and for
    /// coverage (every object named here counts as tested — VGI520).
    pub reference_sql: String,
    /// Optional post-session assertion (grader-only) for non-deterministic tasks.
    pub check_sql: Option<String>,
    /// Optional judge rubric (grader-only).
    pub success_criteria: Option<String>,
    /// Relax strict row-order comparison for this task.
    pub unordered: bool,
}

impl AgentTask {
    /// A minimal exact-compare task (name, prompt, reference_sql).
    pub fn exact(
        name: impl Into<String>,
        prompt: impl Into<String>,
        reference_sql: impl Into<String>,
    ) -> Self {
        AgentTask {
            name: name.into(),
            prompt: prompt.into(),
            reference_sql: reference_sql.into(),
            check_sql: None,
            success_criteria: None,
            unordered: false,
        }
    }

    /// Relax strict row-order comparison for this task.
    pub fn unordered(mut self) -> Self {
        self.unordered = true;
        self
    }
}

/// Build the `vgi.agent_test_tasks` JSON value from a fixed task suite.
pub fn agent_test_tasks_json(tasks: &[AgentTask]) -> String {
    let items: Vec<String> = tasks
        .iter()
        .map(|t| {
            let mut obj = format!(
                "{{\"name\":\"{}\",\"prompt\":\"{}\",\"reference_sql\":\"{}\"",
                json_esc(&t.name),
                json_esc(&t.prompt),
                json_esc(&t.reference_sql)
            );
            if let Some(c) = &t.check_sql {
                obj.push_str(&format!(",\"check_sql\":\"{}\"", json_esc(c)));
            }
            if let Some(s) = &t.success_criteria {
                obj.push_str(&format!(",\"success_criteria\":\"{}\"", json_esc(s)));
            }
            if t.unordered {
                obj.push_str(",\"unordered\":true");
            }
            obj.push('}');
            obj
        })
        .collect();
    format!("[{}]", items.join(","))
}
