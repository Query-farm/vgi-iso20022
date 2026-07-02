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
    pub result_columns_md: &'static str,
}

impl TableFunction for ReadTable {
    fn name(&self) -> &str {
        self.name
    }

    fn metadata(&self) -> FunctionMetadata {
        // doc_md is a richer Markdown section than the one-line `description`, so
        // it adds narrative an agent can't get from the description alone (VGI102).
        let doc_md = format!(
            "# {}\n\n{}\n\nAll reads are local on the worker host — no network, no egress; \
             payment data never leaves the machine. See the result columns for the full schema.",
            self.title, self.doc_llm
        );
        let mut tags = crate::meta::object_tags(self.title, self.doc_llm, &doc_md, self.keywords);
        tags.push(("vgi.category".into(), "Message readers".into()));
        tags.push((
            "vgi.result_columns_md".into(),
            self.result_columns_md.into(),
        ));
        // No `vgi.example_queries` / `vgi.executable_examples`: every `*_read`
        // scans external files, so any example returns zero rows without the data
        // present (VGI902). Usage lives in doc_md + result_columns_md.
        FunctionMetadata {
            description: self.doc_md.to_string(),
            tags,
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::const_arg(
            "path",
            0,
            "any",
            "Path(s) to the message file(s) to read on the worker host. A single file path, or \
             a glob such as '/data/statements/*.xml' to scan every matching file in sorted order \
             (their rows are concatenated). All reads are local — no network, no egress.",
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
