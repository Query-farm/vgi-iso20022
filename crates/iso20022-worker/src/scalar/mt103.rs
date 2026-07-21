//! `iso20022_mt103_field(message, tag) -> VARCHAR` and
//! `iso20022_mt103_amount(message) -> DECIMAL(38,9)`.

use std::sync::Arc;

use arrow_array::builder::{Decimal128Builder, StringBuilder};
use arrow_array::{ArrayRef, RecordBatch};
use arrow_schema::DataType;
use iso20022_core::money::to_decimal128_i128;
use iso20022_core::mt::{self, block};
use vgi::{
    ArgSpec, BindParams, BindResponse, FunctionExample, FunctionMetadata, ProcessParams,
    ScalarFunction,
};
use vgi_rpc::{Result, RpcError};

use crate::arrow_io::{message_bytes_cell, text_str};
use crate::cols::{MONEY_PRECISION, MONEY_SCALE};

pub struct Mt103Field;

impl ScalarFunction for Mt103Field {
    fn name(&self) -> &str {
        "iso20022_mt103_field"
    }

    fn metadata(&self) -> FunctionMetadata {
        let mut tags = crate::meta::object_tags(
            "Read an MT Field by Tag",
            "Return the raw text of any SWIFT MT field by its tag (e.g. '50K', '59', '71F', '32A') \
             from a message's block 4. Repeatable tags (e.g. '71F') are joined with a newline. A \
             bare numeric prefix like '50' matches whichever option variant is present (50A/50F/50K). \
             Returns NULL when the tag is absent or the message is unparseable. Works on any MT \
             type despite the name; the message argument is inline text, a file path, or `BLOB` bytes.",
            "Raw text of an MT field by tag: `iso20022_mt103_field(raw, '50K')`. Repeatable tags \
             are newline-joined; missing tag -> NULL.",
            "mt field, swift tag, 50K, 59, 71F, 32A, raw field, block 4, extract tag, ordering customer",
        );
        tags.push((
            "vgi.executable_examples".into(),
            r#"[{"description":"Read the :50K: ordering customer.","sql":"SELECT iso20022.main.iso20022_mt103_field('{1:F01X}{2:I103X}{4:\n:20:R\n:50K:/123\nACME CORP\n-}', '50K') AS ordering_customer"}]"#.into(),
        ));
        tags.push((
            "vgi.example_queries".into(),
            r#"[{"description":"Read the raw :59: beneficiary field from an inline MT103.","sql":"SELECT iso20022.main.iso20022_mt103_field('{1:F01X}{2:I103X}{4:\n:20:R\n:59:/123\nBENE\n-}', '59');"}]"#.into(),
        ));
        tags.push(("vgi.category".into(), "Message inspection".into()));
        FunctionMetadata {
            description: "Return the raw text of an MT field by tag (newline-joined if repeatable)"
                .into(),
            return_type: Some(DataType::Utf8),
            examples: vec![FunctionExample {
                sql: "SELECT iso20022.main.iso20022_mt103_field('{1:F01X}{2:I103X}{4:\n:20:R\n:59:/123\nBENE\n-}', '59');".into(),
                description: "Read the beneficiary field of each MT103.".into(),
                expected_output: None,
            }],
            tags,
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::any_column(
                "message",
                0,
                "The MT message: inline text, a path to a message file, or BLOB bytes.",
            ),
            ArgSpec::column_typed(
                "tag",
                1,
                DataType::Utf8,
                "The SWIFT field tag to read from message block 4. Tags form an open vocabulary \
                 rather than a fixed set — a two-digit field number with an optional single option \
                 letter. Passing only the digits (for example 50) matches whichever option variant \
                 is present, such as 50A or 50F or 50K; passing the full tag such as 32A reads \
                 exactly that field.",
            ),
        ]
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(DataType::Utf8))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let msg = batch.column(0);
        let tag = batch.column(1);
        let mut out = StringBuilder::new();
        for i in 0..batch.num_rows() {
            let value = match (message_bytes_cell(msg, i).ok().flatten(), text_str(tag, i)?) {
                (Some(bytes), Some(t)) => {
                    let text = String::from_utf8_lossy(&bytes);
                    let b4 = block::parse(&text);
                    mt::raw_tag(&b4, t.trim())
                }
                _ => None,
            };
            match value {
                Some(v) => out.append_value(v),
                None => out.append_null(),
            }
        }
        let arr: ArrayRef = Arc::new(out.finish());
        RecordBatch::try_new(params.output_schema.clone(), vec![arr])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}

pub struct Mt103Amount;

impl ScalarFunction for Mt103Amount {
    fn name(&self) -> &str {
        "iso20022_mt103_amount"
    }

    fn metadata(&self) -> FunctionMetadata {
        let mut tags = crate::meta::object_tags(
            "MT103 Settled Amount",
            "Return the interbank settled amount of an MT103/MT202 message — the `:32A:` amount — \
             as an exact `DECIMAL(38,9)` (comma decimal separator normalized, never a float). Returns \
             NULL when `:32A:` is absent or unparseable. The message argument is inline text, a \
             file path, or `BLOB` bytes.",
            "The exact `:32A:` settled amount of an MT message as `DECIMAL(38,9)`: \
             `iso20022_mt103_amount(raw)`.",
            "mt amount, 32A, settled amount, decimal, exact money, mt103, interbank settlement",
        );
        tags.push((
            "vgi.executable_examples".into(),
            r#"[{"description":"Exact :32A: amount.","sql":"SELECT iso20022.main.iso20022_mt103_amount('{1:F01X}{2:I103X}{4:\n:20:R\n:32A:260101EUR1234,56\n-}') AS amount"}]"#.into(),
        ));
        tags.push((
            "vgi.example_queries".into(),
            r#"[{"description":"Read the exact :32A: interbank settled amount of an inline MT103.","sql":"SELECT iso20022.main.iso20022_mt103_amount('{1:F01X}{2:I103X}{4:\n:20:R\n:32A:260101EUR1234,56\n-}');"}]"#.into(),
        ));
        tags.push(("vgi.category".into(), "Message inspection".into()));
        FunctionMetadata {
            description: "Return the :32A: interbank settled amount as DECIMAL(38,9)".into(),
            return_type: Some(DataType::Decimal128(MONEY_PRECISION, MONEY_SCALE)),
            examples: vec![FunctionExample {
                sql: "SELECT iso20022.main.iso20022_mt103_amount('{1:F01X}{2:I103X}{4:\n:20:R\n:32A:260101EUR1234,56\n-}');".into(),
                description: "Read the settled amount of each message.".into(),
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
            "The MT message: inline text, a path to a message file, or BLOB bytes.",
        )]
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(DataType::Decimal128(
            MONEY_PRECISION,
            MONEY_SCALE,
        )))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let msg = batch.column(0);
        let mut b = Decimal128Builder::new();
        for i in 0..batch.num_rows() {
            let amount = message_bytes_cell(msg, i)
                .ok()
                .flatten()
                .and_then(|bytes| {
                    let text = String::from_utf8_lossy(&bytes);
                    let b4 = block::parse(&text);
                    b4.first("32A").map(mt::field::parse_32a)
                })
                .and_then(|a| a.amount)
                .and_then(to_decimal128_i128);
            match amount {
                Some(m) => b.append_value(m),
                None => b.append_null(),
            }
        }
        let arr: ArrayRef = Arc::new(
            b.finish()
                .with_precision_and_scale(MONEY_PRECISION, MONEY_SCALE)
                .map_err(|e| RpcError::runtime_error(e.to_string()))?,
        );
        RecordBatch::try_new(params.output_schema.clone(), vec![arr])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrow_io::test_support::{process_params, text_batch};
    use arrow_array::cast::AsArray;
    use arrow_array::types::Decimal128Type;
    use arrow_array::{Array, StringArray};
    use arrow_schema::{Field, Schema};
    use vgi::arguments::Arguments;

    const MT103: &str =
        "{1:F01X}{2:I103X}{4:\n:20:R\n:32A:260101EUR1234,56\n:50K:/123\nACME CORP\n-}";

    #[test]
    fn reads_field() {
        let msg: ArrayRef = Arc::new(StringArray::from(vec![Some(MT103)]));
        let tag: ArrayRef = Arc::new(StringArray::from(vec![Some("50K")]));
        let schema = Arc::new(Schema::new(vec![
            Field::new("message", DataType::Utf8, true),
            Field::new("tag", DataType::Utf8, true),
        ]));
        let batch = RecordBatch::try_new(schema.clone(), vec![msg, tag]).unwrap();
        let bind = BindParams {
            input_schema: Some(schema),
            ..Default::default()
        };
        let bound = Mt103Field.on_bind(&bind).unwrap();
        let params = process_params(bound.output_schema, Arguments::default());
        let out = Mt103Field.process(&params, &batch).unwrap();
        assert_eq!(out.column(0).as_string::<i32>().value(0), "/123\nACME CORP");
    }

    #[test]
    fn reads_amount() {
        let batch = text_batch("message", &[Some(MT103), None]);
        let bind = BindParams {
            input_schema: Some(batch.schema()),
            ..Default::default()
        };
        let bound = Mt103Amount.on_bind(&bind).unwrap();
        let params = process_params(bound.output_schema, Arguments::default());
        let out = Mt103Amount.process(&params, &batch).unwrap();
        let d = out.column(0).as_primitive::<Decimal128Type>();
        assert_eq!(d.value(0), 1_234_560_000_000); // 1234.56 * 10^9
        assert!(out.column(0).is_null(1));
    }
}
