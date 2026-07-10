//! A single generic [`PerMessageTable`] implements [`TableInOutFunction`] for the
//! per-message child functions (`*_entries`, `*_lines`).
//!
//! Each takes an **input relation** whose `msg :=` column (default `raw`) holds a
//! parent message, parses every input row, and streams the child rows out — with
//! **every input column passed through** (repeated once per child) so results
//! correlate straight back to the parent. This is the DuckDB-supported,
//! sourcemap/sklearn table-in-out convention (a correlated `LATERAL tf(s.raw)`
//! scalar parameter is NOT supported by DuckDB in-out functions):
//!
//! ```sql
//! SELECT account_iban, amount, credit_debit
//! FROM iso20022.main.camt053_entries(
//!        (SELECT account_iban, raw FROM iso20022.main.camt053_read('/data/*.xml')));
//! ```
//!
//! Per-row safety: a malformed / empty / NULL message simply yields no child rows
//! for that input row — it never fails the query.

use std::sync::Arc;

use arrow_array::{ArrayRef, RecordBatch, UInt32Array};
use arrow_schema::{Field, Schema, SchemaRef};
use vgi::arguments::Arguments;
use vgi::function::{ArgSpec, BindParams, BindResponse, FunctionMetadata, ProcessParams};
use vgi::table_in_out::TableInOutFunction;
use vgi_rpc::{Result, RpcError};

use crate::arrow_io::message_bytes_cell;

/// Build child columns from the input messages: returns the per-message child
/// **counts** (so the parent columns can be repeated to match) and the child
/// column arrays (concatenated across all messages).
pub type ChildBuildFn = fn(&[String]) -> (Vec<usize>, Vec<ArrayRef>);

/// One per-message child table-in-out function.
pub struct PerMessageTable {
    pub name: &'static str,
    /// The child-only field schema (appended after the passthrough columns).
    pub child_schema: fn() -> SchemaRef,
    pub build: ChildBuildFn,
    pub title: &'static str,
    pub doc_llm: &'static str,
    pub doc_md: &'static str,
    pub keywords: &'static str,
    pub executable_examples: &'static str,
}

/// The name of the input column holding the message (default `raw`).
fn msg_name(args: &Arguments) -> String {
    args.named_str("msg").unwrap_or_else(|| "raw".to_string())
}

impl TableInOutFunction for PerMessageTable {
    fn name(&self) -> &str {
        self.name
    }

    fn metadata(&self) -> FunctionMetadata {
        // doc_md is a richer Markdown section than the one-line `description`, so
        // it adds narrative an agent can't get from the description alone (VGI102).
        let doc_md = format!(
            "# {}\n\n{}\n\n## Calling it\n\nThis is a table-in-out function: pass the relation to \
             explode as a subquery, e.g. `{}((SELECT raw FROM <reader>('…')))`, or feed it any \
             relation that exposes a `raw` message column (rename it with `msg := 'col'`). Every \
             input column is passed through unchanged, repeated once per child row, so results \
             correlate back to the parent.\n\n## Result schema\n\nThe passthrough input columns \
             come first, followed by the fixed child columns — their names, types, and meanings \
             are documented in `vgi.result_dynamic_columns_md`, and a runnable call is in \
             `vgi.executable_examples`. Local-only — no network, no egress.",
            self.title, self.doc_llm, self.name
        );
        let mut tags = crate::meta::object_tags(self.title, self.doc_llm, &doc_md, self.keywords);
        tags.push(("vgi.category".into(), "Statement exploders".into()));
        // The result schema is dynamic (passthrough input columns ++ fixed child
        // columns), so it is declared as `vgi.result_dynamic_columns_md`, generated
        // from the child Arrow schema so the variant table can never drift from the
        // emitted child columns (VGI307/326/322/323/414/910).
        tags.push((
            "vgi.result_dynamic_columns_md".into(),
            crate::meta::result_dynamic_columns_md(&(self.child_schema)()),
        ));
        tags.push((
            "vgi.executable_examples".into(),
            self.executable_examples.into(),
        ));
        FunctionMetadata {
            description: self.doc_md.to_string(),
            tags,
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::column(
                "input",
                0,
                "table",
                "The input relation to explode, supplied as a subquery, e.g. \
                 `(SELECT account_iban, raw FROM camt053_read('/data/*.xml'))`. It must contain the \
                 message column named by `msg` (default 'raw'); every input column is passed through \
                 to the output (repeated once per child row) so results correlate back to the parent.",
            ),
            ArgSpec::const_arg(
                "msg",
                -1,
                "varchar",
                "Name of the input column holding the parent message (the `raw` text of a \
                 statement / notification). Default 'raw'.",
            ),
        ]
    }

    fn on_bind(&self, params: &BindParams) -> Result<BindResponse> {
        let input = params.input_schema.clone().ok_or_else(|| {
            RpcError::value_error(format!("{} requires an input relation", self.name))
        })?;
        let msg = msg_name(&params.arguments);
        if input.column_with_name(&msg).is_none() {
            return Err(RpcError::value_error(format!(
                "{}: message column '{}' not found in the input relation",
                self.name, msg
            )));
        }
        // Output = passthrough input fields ++ child fields.
        let mut fields: Vec<Field> = input.fields().iter().map(|f| f.as_ref().clone()).collect();
        for f in (self.child_schema)().fields() {
            fields.push(f.as_ref().clone());
        }
        Ok(BindResponse {
            output_schema: Arc::new(Schema::new(fields)),
            opaque_data: Vec::new(),
        })
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<Vec<RecordBatch>> {
        let msg = msg_name(&params.arguments);
        let msg_col = batch
            .column_by_name(&msg)
            .ok_or_else(|| RpcError::runtime_error(format!("missing message column '{msg}'")))?;

        // Read one message per input row (NULL / unreadable -> empty -> 0 children).
        let mut messages = Vec::with_capacity(batch.num_rows());
        for i in 0..batch.num_rows() {
            match message_bytes_cell(msg_col, i).ok().flatten() {
                Some(bytes) => messages.push(String::from_utf8_lossy(&bytes).into_owned()),
                None => messages.push(String::new()),
            }
        }

        let (counts, child_cols) = (self.build)(&messages);

        // Repeat each parent row index once per child it produced, then gather the
        // passthrough columns with that index so each child carries its parent row.
        let mut take_idx: Vec<u32> = Vec::new();
        for (row, &k) in counts.iter().enumerate() {
            for _ in 0..k {
                take_idx.push(row as u32);
            }
        }
        let indices = UInt32Array::from(take_idx);
        let mut columns: Vec<ArrayRef> = Vec::with_capacity(batch.num_columns() + child_cols.len());
        for c in batch.columns() {
            let taken = arrow_select::take::take(c, &indices, None)
                .map_err(|e| RpcError::runtime_error(format!("passthrough take: {e}")))?;
            columns.push(taken);
        }
        columns.extend(child_cols);

        let rb = RecordBatch::try_new(params.output_schema.clone(), columns)
            .map_err(|e| RpcError::runtime_error(e.to_string()))?;
        Ok(vec![rb])
    }
}
