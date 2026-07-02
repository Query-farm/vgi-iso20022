//! `iso20022_version()` — return the worker's version string.

use std::sync::Arc;

use arrow_array::{ArrayRef, RecordBatch, StringArray};
use arrow_schema::DataType;
use vgi::{
    ArgSpec, BindParams, BindResponse, FunctionExample, FunctionMetadata, ProcessParams,
    ScalarFunction,
};
use vgi_rpc::{Result, RpcError};

pub struct Iso20022Version;

impl ScalarFunction for Iso20022Version {
    fn name(&self) -> &str {
        "iso20022_version"
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description: "Returns the iso20022 worker version string".into(),
            return_type: Some(DataType::Utf8),
            examples: vec![FunctionExample {
                sql: "SELECT iso20022.main.iso20022_version();".into(),
                description: "Return the iso20022 worker version string.".into(),
                expected_output: None,
            }],
            tags: {
                let mut t = crate::meta::object_tags(
                    "ISO 20022 Worker Version",
                    "Return the semantic version string of the running iso20022 worker binary. \
                     Useful for diagnostics and confirming which build is attached.",
                    "Return the iso20022 worker version, e.g. `iso20022_version()` -> '0.1.0'.",
                    "version, build version, iso20022_version, diagnostics, worker version, semver",
                );
                t.push((
                    "vgi.executable_examples".into(),
                    r#"[{"description":"Worker version.","sql":"SELECT iso20022.main.iso20022_version() AS version"}]"#.into(),
                ));
                t.push(("vgi.category".into(), "Message inspection".into()));
                t
            },
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        Vec::new()
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(DataType::Utf8))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let rows = batch.num_rows();
        let out: ArrayRef = Arc::new(StringArray::from(vec![crate::version(); rows]));
        RecordBatch::try_new(params.output_schema.clone(), vec![out])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}
