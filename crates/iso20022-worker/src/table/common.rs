//! A single generic [`ReadTable`] implements [`TableFunction`] for every
//! `*_read` file-glob scan; each message module supplies only its schema, its
//! per-file build function, and its documentation strings.

use arrow_schema::SchemaRef;
use vgi::table_function::{TableFunction, TableProducer};
use vgi::{ArgSpec, BindParams, BindResponse, FunctionMetadata, ProcessParams};
use vgi_rpc::Result;

use super::scan::{resolve_paths, BuildFn, GlobProducer};

/// One file-glob table function, parameterized by its message type.
pub struct ReadTable {
    pub name: &'static str,
    pub schema: fn() -> SchemaRef,
    pub build: BuildFn,
    pub title: &'static str,
    pub doc_llm: &'static str,
    pub doc_md: &'static str,
    pub keywords: &'static str,
    /// A self-contained `vgi.executable_examples` JSON array (must-run). Each
    /// reader is demonstrated on inline message text so the example returns rows
    /// without a data file present (VGI511/VGI906).
    pub executable_examples: &'static str,
}

impl TableFunction for ReadTable {
    fn name(&self) -> &str {
        self.name
    }

    fn metadata(&self) -> FunctionMetadata {
        // doc_md is a richer Markdown section than the one-line `description`, so
        // it adds narrative an agent can't get from the description alone (VGI102).
        let doc_md = format!(
            "# {}\n\n{}\n\nThe argument may be the **message text supplied inline** (handy for a \
             single message or an example), a file path, or a glob that scans many files. All \
             reads are local on the worker host — no network, no egress; payment data never leaves \
             the machine. See the result columns for the full schema.",
            self.title, self.doc_llm
        );
        let mut tags = crate::meta::object_tags(self.title, self.doc_llm, &doc_md, self.keywords);
        tags.push(("vgi.category".into(), "Message readers".into()));
        // The result schema is static (same columns regardless of argument) and is
        // generated straight from the Arrow schema, so the declared shape can never
        // drift from what the batch actually carries (VGI307/321/322/323/910).
        tags.push((
            "vgi.result_columns_schema".into(),
            crate::meta::result_columns_schema_json(&(self.schema)()),
        ));
        // Demonstrated on inline message text (a reader also accepts inline content,
        // not just a file path), so the example returns rows at lint time (VGI511).
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
        vec![ArgSpec::const_arg(
            "source",
            0,
            "any",
            "The message source to read. Either the message text supplied inline (MX XML, or an \
             MT `{…}` FIN message), or a path to a message file on the worker host, or a glob such \
             as '/data/statements/*.xml' that scans every matching file in sorted order (their rows \
             are concatenated). All reads are local — no network, no egress.",
        )]
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse {
            output_schema: (self.schema)(),
            opaque_data: Vec::new(),
        })
    }

    fn producer(&self, params: &ProcessParams) -> Result<Box<dyn TableProducer>> {
        let paths = resolve_paths(&params.arguments)?;
        Ok(Box::new(GlobProducer::new(
            (self.schema)(),
            paths,
            self.build,
        )))
    }
}
