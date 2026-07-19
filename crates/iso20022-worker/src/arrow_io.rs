//! Arrow boundary helpers: read VARCHAR/BLOB cells, and resolve the overloaded
//! **message** argument (path / inline text / bytes) the same way across every
//! scalar and per-message table-in-out function.

use arrow_array::cast::AsArray;
use arrow_array::{Array, ArrayRef};
use arrow_schema::DataType;
use vgi_rpc::{Result, RpcError};

/// Borrow the UTF-8 text of a VARCHAR cell at `row`, or `None` if null.
pub fn text_str(col: &ArrayRef, row: usize) -> Result<Option<String>> {
    if col.is_null(row) {
        return Ok(None);
    }
    Ok(Some(match col.data_type() {
        DataType::Utf8 => col.as_string::<i32>().value(row).to_string(),
        DataType::LargeUtf8 => col.as_string::<i64>().value(row).to_string(),
        DataType::Utf8View => col.as_string_view().value(row).to_string(),
        other => {
            return Err(RpcError::value_error(format!(
                "expected a VARCHAR argument, got {other:?}"
            )))
        }
    }))
}

/// Does this VARCHAR look like an inline message rather than a file path? MX XML
/// starts with `<`; MT starts with a `{…}` block or a bare `:NN:` tag stream; a
/// multi-line value is also treated as inline content.
pub fn looks_like_message(s: &str) -> bool {
    let t = s.trim_start();
    t.starts_with('<') || t.starts_with('{') || t.starts_with(':') || t.contains('\n')
}

/// Resolve a message cell to raw bytes: inline text → its own bytes; a path →
/// the file's bytes; a BLOB → verbatim. `None` if the cell is null. Errors only
/// on an unreadable path or an unsupported column type — callers decide whether
/// that is a per-row NULL or a hard error.
pub fn message_bytes_cell(col: &ArrayRef, row: usize) -> Result<Option<Vec<u8>>> {
    if col.is_null(row) {
        return Ok(None);
    }
    let bytes = match col.data_type() {
        DataType::Utf8 => str_to_bytes(col.as_string::<i32>().value(row))?,
        DataType::LargeUtf8 => str_to_bytes(col.as_string::<i64>().value(row))?,
        DataType::Utf8View => str_to_bytes(col.as_string_view().value(row))?,
        DataType::Binary => col.as_binary::<i32>().value(row).to_vec(),
        DataType::LargeBinary => col.as_binary::<i64>().value(row).to_vec(),
        DataType::BinaryView => col.as_binary_view().value(row).to_vec(),
        other => {
            return Err(RpcError::value_error(format!(
                "the message argument must be a VARCHAR (inline message or path) or BLOB, got {other:?}"
            )))
        }
    };
    Ok(Some(bytes))
}

/// Resolve a VARCHAR cell to bytes: inline message text → its own bytes;
/// otherwise read the file at that path.
fn str_to_bytes(s: &str) -> Result<Vec<u8>> {
    if looks_like_message(s) {
        Ok(s.as_bytes().to_vec())
    } else {
        std::fs::read(s).map_err(|e| RpcError::value_error(format!("message path '{s}': {e}")))
    }
}

#[cfg(test)]
pub mod test_support {
    use std::sync::Arc;

    use arrow_array::builder::StringBuilder;
    use arrow_array::{ArrayRef, RecordBatch};
    use arrow_schema::{Field, Schema, SchemaRef};
    use vgi::arguments::Arguments;
    use vgi::{BindParams, ProcessParams, ScalarFunction};
    use vgi_rpc::Result;

    /// A single-column `Utf8` (VARCHAR) input batch.
    pub fn text_batch(name: &str, rows: &[Option<&str>]) -> RecordBatch {
        let mut b = StringBuilder::new();
        for r in rows {
            match r {
                Some(s) => b.append_value(s),
                None => b.append_null(),
            }
        }
        let arr: ArrayRef = Arc::new(b.finish());
        let schema = Arc::new(Schema::new(vec![Field::new(
            name,
            arr.data_type().clone(),
            true,
        )]));
        RecordBatch::try_new(schema, vec![arr]).unwrap()
    }

    /// Build a `ProcessParams` carrying the given output schema and arguments.
    pub fn process_params(output_schema: SchemaRef, arguments: Arguments) -> ProcessParams {
        ProcessParams {
            substream_id: None,
            if_none_match: None,
            if_modified_since: None,
            output_schema,
            input_schema: None,
            execution_id: Vec::new(),
            init_opaque_data: Vec::new(),
            arguments,
            settings: Default::default(),
            secrets: Default::default(),
            auth_principal: None,
            projection_ids: None,
            pushdown_filters: None,
            join_keys: Vec::new(),
            storage: None,
            order_by_column: None,
            order_by_direction: None,
            order_by_null_order: None,
            order_by_limit: None,
            tablesample_percentage: None,
            tablesample_seed: None,
            attach_opaque_data: None,
            at_unit: None,
            at_value: None,
            copy_from: None,
        }
    }

    /// Run a scalar over a single-column VARCHAR batch.
    pub fn run_scalar_text<F: ScalarFunction>(
        f: &F,
        col_name: &str,
        rows: &[Option<&str>],
        arguments: Arguments,
    ) -> Result<ArrayRef> {
        let batch = text_batch(col_name, rows);
        let bind = BindParams {
            input_schema: Some(batch.schema()),
            arguments: arguments.clone(),
            ..Default::default()
        };
        let bound = f.on_bind(&bind)?;
        let params = process_params(bound.output_schema.clone(), arguments);
        Ok(f.process(&params, &batch)?.column(0).clone())
    }
}
