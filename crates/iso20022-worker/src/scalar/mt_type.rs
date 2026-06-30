//! `iso20022_mt_type(message) -> VARCHAR` — sniff a SWIFT MT message's type
//! label (e.g. `'MT103'`), or NULL for an ISO 20022 MX / unrecognized message.

use std::sync::Arc;

use arrow_array::builder::StringBuilder;
use arrow_array::{ArrayRef, RecordBatch};
use arrow_schema::DataType;
use iso20022_core::sniff;
use vgi::{
    ArgSpec, BindParams, BindResponse, FunctionExample, FunctionMetadata, ProcessParams,
    ScalarFunction,
};
use vgi_rpc::{Result, RpcError};

use crate::arrow_io::message_bytes_cell;

pub struct MtType;

impl ScalarFunction for MtType {
    fn name(&self) -> &str {
        "iso20022_mt_type"
    }

    fn metadata(&self) -> FunctionMetadata {
        let mut tags = crate::meta::object_tags(
            "Sniff SWIFT MT Type",
            "Detect the SWIFT MT message type of a raw payment message and return its label \
             (e.g. 'MT103', 'MT202', 'MT940', 'MT942'), reading the block-2 application header. \
             Returns NULL when the input is an ISO 20022 MX (XML) message or is not a recognizable \
             MT message. The message argument is a VARCHAR holding the message text inline, a \
             VARCHAR path to a message file on the worker host, or the raw BLOB bytes.",
            "Return the SWIFT MT type label of a message, e.g. `iso20022_mt_type(raw)` -> 'MT103'; \
             NULL for MX/unknown input. Accepts inline text, a file path, or BLOB bytes.",
            "mt type, swift mt, sniff, detect message type, mt103, mt202, mt940, mt942, message kind",
        );
        tags.push((
            "vgi.executable_examples".into(),
            r#"[{"description":"Sniff an inline MT103.","sql":"SELECT iso20022.main.iso20022_mt_type('{1:F01BANKBEBBAXXX0000000000}{2:I103DEUTDEFFXXXXN}{4:\n:20:R\n:32A:260101EUR1,00\n-}') AS mt"}]"#.into(),
        ));
        FunctionMetadata {
            description: "Detect a SWIFT MT message's type label (NULL for MX/unknown)".into(),
            return_type: Some(DataType::Utf8),
            examples: vec![FunctionExample {
                sql: "SELECT iso20022.main.iso20022_mt_type('{1:F01X}{2:I103DEUTDEFFXXXXN}{4:\n:20:R\n-}');".into(),
                description: "Sniff the MT type of an inline message.".into(),
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
            "The payment message: the raw MT/MX text inline, a path to a message file on the \
             worker host, or the message BLOB bytes.",
        )]
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(DataType::Utf8))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let col = batch.column(0);
        let mut out = StringBuilder::new();
        for i in 0..batch.num_rows() {
            match message_bytes_cell(col, i).ok().flatten() {
                Some(bytes) => match sniff::mt_type_label(&bytes) {
                    Some(label) => out.append_value(label),
                    None => out.append_null(),
                },
                None => out.append_null(),
            }
        }
        let arr: ArrayRef = Arc::new(out.finish());
        RecordBatch::try_new(params.output_schema.clone(), vec![arr])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrow_io::test_support::run_scalar_text;
    use arrow_array::cast::AsArray;
    use arrow_array::Array;
    use vgi::arguments::Arguments;

    #[test]
    fn sniffs_mt_and_nulls_mx() {
        let mt = "{1:F01X}{2:I103X}{4:\n:20:R\n-}";
        let mx = "<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08\"><FIToFICstmrCdtTrf/></Document>";
        let out = run_scalar_text(
            &MtType,
            "message",
            &[Some(mt), Some(mx), None],
            Arguments::default(),
        )
        .unwrap();
        let s = out.as_string::<i32>();
        assert_eq!(s.value(0), "MT103");
        assert!(out.is_null(1), "MX -> NULL");
        assert!(out.is_null(2), "NULL -> NULL");
    }
}
