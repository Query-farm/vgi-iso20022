//! `iso20022_validate(message) -> STRUCT(ok BOOLEAN, errors VARCHAR[])`.
//!
//! Auto-detects MT vs MX, runs structural + selected CBPR+ field rules, and
//! returns a fixed-schema struct: `ok` is true iff `errors` is empty. Each error
//! is a stable `CODE: human text` string.

use std::sync::Arc;

use arrow_array::builder::{BooleanBuilder, ListBuilder, StringBuilder};
use arrow_array::{ArrayRef, RecordBatch, StructArray};
use arrow_schema::{DataType, Field, Fields};
use iso20022_core::validate::validate;
use vgi::{
    ArgSpec, BindParams, BindResponse, FunctionExample, FunctionMetadata, ProcessParams,
    ScalarFunction,
};
use vgi_rpc::{Result, RpcError};

use crate::arrow_io::message_bytes_cell;
use crate::cols::list_utf8_type;

pub struct Validate;

/// The fixed `STRUCT(ok BOOLEAN, errors VARCHAR[])` output fields.
fn struct_fields() -> Fields {
    Fields::from(vec![
        Field::new("ok", DataType::Boolean, false),
        Field::new("errors", list_utf8_type(), false),
    ])
}

impl ScalarFunction for Validate {
    fn name(&self) -> &str {
        "iso20022_validate"
    }

    fn metadata(&self) -> FunctionMetadata {
        let mut tags = crate::meta::object_tags(
            "Validate a Payment Message",
            "Validate a SWIFT MT or ISO 20022 MX payment message and return a STRUCT(ok BOOLEAN, \
             errors VARCHAR[]). It auto-detects the family and message type, then runs structural \
             checks (balanced MT blocks / well-formed MX, mandatory tags & elements present) plus \
             selected CBPR+ SR2025 field rules: BIC format, IBAN mod-97 checksum, ISO 4217 currency \
             and minor-unit consistency, charge-bearer code sets, and UETR UUID shape. `ok` is true \
             iff `errors` is empty; each error is a stable 'CODE: text' string you can unnest or \
             list_contains. This is structural + field-level validation, NOT full CBPR+/HVPS+ \
             network validation. The message argument is inline text, a file path, or BLOB bytes.",
            "Validate an MT/MX message -> `STRUCT(ok BOOLEAN, errors VARCHAR[])` (structural + \
             IBAN/BIC/currency/charge-bearer/UETR field rules). `ok` is true iff `errors` is empty.",
            "validate, validation, iso 20022 validate, cbpr+, iban checksum, bic format, currency, \
             charge bearer, uetr, structural check, mandatory fields, sr2025",
        );
        tags.push((
            "vgi.executable_examples".into(),
            r#"[{"description":"Validate an inline MT103.","sql":"SELECT (iso20022.main.iso20022_validate('{1:F01X}{2:I103X}{4:\n:20:R\n:23B:CRED\n:32A:260101EUR1,00\n:50K:/DE89370400440532013000\nACME\n:59:/FR1420041010050500013M02606\nWIDGETS\n:71A:SHA\n-}')).ok AS ok"}]"#.into(),
        ));
        FunctionMetadata {
            description: "Validate an MT/MX message: STRUCT(ok BOOLEAN, errors VARCHAR[])".into(),
            return_type: Some(DataType::Struct(struct_fields())),
            examples: vec![FunctionExample {
                sql: "SELECT (iso20022.main.iso20022_validate('{1:F01X}{2:I103X}{4:\n:20:R\n:32A:260101EUR1,00\n-}')).ok;".into(),
                description: "Validate an inline message and read the ok flag.".into(),
                expected_output: None,
            }],
            tags,
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::any_column(
            "message",
            0,
            "The payment message to validate: inline MT/MX text, a path to a message file, or the \
             message BLOB bytes.",
        )]
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(DataType::Struct(struct_fields())))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let col = batch.column(0);
        let mut ok = BooleanBuilder::new();
        let mut errors = ListBuilder::new(StringBuilder::new());

        for i in 0..batch.num_rows() {
            match message_bytes_cell(col, i).ok().flatten() {
                Some(bytes) => {
                    let v = validate(&bytes);
                    ok.append_value(v.ok);
                    for e in &v.errors {
                        errors.values().append_value(e);
                    }
                    errors.append(true);
                }
                None => {
                    // NULL input -> ok=false with a single explanatory error.
                    ok.append_value(false);
                    errors
                        .values()
                        .append_value("STRUCT000: NULL or unreadable message");
                    errors.append(true);
                }
            }
        }

        let ok_arr: ArrayRef = Arc::new(ok.finish());
        let err_arr: ArrayRef = Arc::new(errors.finish());
        let struct_arr = StructArray::try_new(struct_fields(), vec![ok_arr, err_arr], None)
            .map_err(|e| RpcError::runtime_error(e.to_string()))?;
        let out: ArrayRef = Arc::new(struct_arr);
        RecordBatch::try_new(params.output_schema.clone(), vec![out])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrow_io::test_support::{process_params, text_batch};
    use arrow_array::cast::AsArray;
    use arrow_array::Array;
    use vgi::arguments::Arguments;

    fn run(msg: Option<&str>) -> (bool, usize) {
        let batch = text_batch("message", &[msg]);
        let bind = BindParams {
            input_schema: Some(batch.schema()),
            ..Default::default()
        };
        let bound = Validate.on_bind(&bind).unwrap();
        let params = process_params(bound.output_schema, Arguments::default());
        let out = Validate.process(&params, &batch).unwrap();
        let st = out.column(0).as_struct();
        let ok = st.column(0).as_boolean().value(0);
        let errs = st.column(1).as_list::<i32>();
        (ok, errs.value(0).len())
    }

    #[test]
    fn good_message_ok() {
        let msg = "{1:F01X}{2:I103X}{3:{121:e3bf1c2a-1111-4aaa-8bbb-1234567890ab}}{4:\n:20:R\n:23B:CRED\n:32A:260101EUR1,00\n:50K:/DE89370400440532013000\nACME\n:59:/FR1420041010050500013M02606\nWIDGETS\n:71A:SHA\n-}";
        let (ok, n) = run(Some(msg));
        assert!(ok, "expected ok");
        assert_eq!(n, 0);
    }

    #[test]
    fn broken_message_flags() {
        let msg = "{1:F01X}{2:I103X}{4:\n:20:R\n:71A:ZZZ\n-}";
        let (ok, n) = run(Some(msg));
        assert!(!ok);
        assert!(n > 0);
    }
}
