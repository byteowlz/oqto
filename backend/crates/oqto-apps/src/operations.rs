//! Immutable operations table shipped inside an App package.
//!
//! The table names semantic operations and the package-relative executable
//! that implements each one. This crate parses only what publication must
//! prove: that every requested operation exists, and that its `exec[0]` is a
//! safe package-relative file under `operations/`. Execution policy (stdin,
//! stdout framing, timeouts, parameter schemas) is preserved verbatim for the
//! runner-side Gate and deliberately not interpreted here.
//!
//! Everything under `operations/` is content-addressed with the Definition, so
//! editing a bridge script after approval produces a different Definition
//! rather than silently changing what an existing grant authorizes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::capability::{
    MAX_OPERATION_ID_BYTES, OPERATIONS_DIR, PACKAGE_PATH_LIMITS, is_valid_operation_id,
};
use crate::path::{bounded_relative_path, is_beneath};
use crate::{AppPackageError, AppPackageErrorCode};

pub const OPERATIONS_SCHEMA_V0_DRAFT: &str = "oqto-app-operations/v0-draft";

const MAX_OPERATIONS: usize = 128;
const MAX_SUMMARY_BYTES: usize = 256;
const MAX_EXEC_ARGS: usize = 16;
const MAX_EXEC_ARG_BYTES: usize = 256;

/// One operation definition with the fields publication must understand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationDefinition {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Package-relative executable implementing the operation.
    pub executable: PathBuf,
    /// Fixed arguments appended after the executable. Never a shell string.
    pub args: Vec<String>,
}

/// A parsed, validated operations table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsTable {
    pub schema: String,
    pub operations: Vec<OperationDefinition>,
}

/// One operation a Definition actually exposes, after intersecting the table
/// with the manifest's requested ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedOperation {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub executable: PathBuf,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct RawOperationsFile {
    schema: String,
    #[serde(default)]
    operation: Vec<RawOperation>,
    /// Draft schema: unknown top-level keys are preserved, never interpreted.
    #[serde(flatten)]
    #[allow(dead_code)]
    extra: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct RawOperation {
    id: String,
    #[serde(default)]
    summary: Option<String>,
    exec: Vec<String>,
    /// Execution policy consumed by the runner Gate, not by publication.
    #[serde(flatten)]
    #[allow(dead_code)]
    extra: BTreeMap<String, toml::Value>,
}

impl OperationsTable {
    /// Resolve the manifest's requested ids against this table.
    ///
    /// Every requested id must exist; unrequested table entries are ignored
    /// because they are not grantable.
    pub fn resolve(&self, requested: &[String]) -> Result<Vec<ResolvedOperation>, AppPackageError> {
        let mut resolved = Vec::with_capacity(requested.len());
        for id in requested {
            let definition = self
                .operations
                .iter()
                .find(|operation| &operation.id == id)
                .ok_or_else(|| {
                    AppPackageError::new(
                        AppPackageErrorCode::OperationMissing,
                        format!(
                            "requested operation {id:?} is not defined in the operations table"
                        ),
                    )
                })?;
            resolved.push(ResolvedOperation {
                id: definition.id.clone(),
                summary: definition.summary.clone(),
                executable: definition.executable.clone(),
                args: definition.args.clone(),
            });
        }
        Ok(resolved)
    }
}

/// Parse and validate an operations table.
///
/// `max_bytes` bounds the table itself; the file is already bounded by the
/// per-file publication limit, but a table is configuration and must stay
/// small enough to review.
pub fn parse_operations_table(
    bytes: &[u8],
    max_bytes: u64,
    table_path: &Path,
) -> Result<OperationsTable, AppPackageError> {
    let byte_len = u64::try_from(bytes.len())
        .map_err(|_| invalid_table(table_path, "operations table length cannot be represented"))?;
    if byte_len > max_bytes {
        return Err(invalid_table(
            table_path,
            format!("operations table is {byte_len} bytes; limit is {max_bytes}"),
        ));
    }

    let text = std::str::from_utf8(bytes)
        .map_err(|_| invalid_table(table_path, "operations table must be UTF-8"))?;
    let raw: RawOperationsFile = toml::from_str(text)
        .map_err(|error| invalid_table(table_path, format!("invalid operations TOML: {error}")))?;

    if raw.schema != OPERATIONS_SCHEMA_V0_DRAFT {
        return Err(invalid_table(
            table_path,
            format!(
                "unsupported operations schema {:?}; expected {OPERATIONS_SCHEMA_V0_DRAFT:?}",
                raw.schema
            ),
        ));
    }
    if raw.operation.is_empty() {
        return Err(invalid_table(
            table_path,
            "operations table defines no operations",
        ));
    }
    if raw.operation.len() > MAX_OPERATIONS {
        return Err(invalid_table(
            table_path,
            format!(
                "operations table defines {} operations; limit is {MAX_OPERATIONS}",
                raw.operation.len()
            ),
        ));
    }

    let mut ids = BTreeSet::new();
    let mut operations = Vec::with_capacity(raw.operation.len());
    for entry in &raw.operation {
        if !is_valid_operation_id(&entry.id) {
            return Err(invalid_table(
                table_path,
                format!(
                    "operation id {:?} is not a valid dotted identifier of at most \
                     {MAX_OPERATION_ID_BYTES} bytes",
                    entry.id
                ),
            ));
        }
        if !ids.insert(entry.id.clone()) {
            return Err(invalid_table(
                table_path,
                format!("operation id {:?} is defined more than once", entry.id),
            ));
        }
        if let Some(summary) = &entry.summary
            && summary.len() > MAX_SUMMARY_BYTES
        {
            return Err(invalid_table(
                table_path,
                format!(
                    "operation {:?} summary is {} bytes; limit is {MAX_SUMMARY_BYTES}",
                    entry.id,
                    summary.len()
                ),
            ));
        }

        let (executable, args) = validate_exec(&entry.id, &entry.exec, table_path)?;
        operations.push(OperationDefinition {
            id: entry.id.clone(),
            summary: entry.summary.clone(),
            executable,
            args,
        });
    }

    Ok(OperationsTable {
        schema: raw.schema,
        operations,
    })
}

/// `exec[0]` must be a package-relative executable under `operations/`; the
/// runner never resolves it through `PATH` and never accepts a host path.
fn validate_exec(
    id: &str,
    exec: &[String],
    table_path: &Path,
) -> Result<(PathBuf, Vec<String>), AppPackageError> {
    let Some((program, args)) = exec.split_first() else {
        return Err(invalid_table(
            table_path,
            format!("operation {id:?} must declare a non-empty exec argv"),
        ));
    };
    if exec.len() > MAX_EXEC_ARGS {
        return Err(invalid_table(
            table_path,
            format!(
                "operation {id:?} declares {} argv entries; limit is {MAX_EXEC_ARGS}",
                exec.len()
            ),
        ));
    }

    let executable = bounded_relative_path(program, PACKAGE_PATH_LIMITS).map_err(|error| {
        invalid_table(table_path, format!("operation {id:?}: {}", error.message))
    })?;
    if !is_beneath(&executable, OPERATIONS_DIR) {
        return Err(invalid_table(
            table_path,
            format!(
                "operation {id:?} exec[0] must be a package-relative file under {OPERATIONS_DIR}/"
            ),
        ));
    }

    for argument in args {
        if argument.len() > MAX_EXEC_ARG_BYTES {
            return Err(invalid_table(
                table_path,
                format!(
                    "operation {id:?} argument is {} bytes; limit is {MAX_EXEC_ARG_BYTES}",
                    argument.len()
                ),
            ));
        }
        if argument.chars().any(char::is_control) {
            return Err(invalid_table(
                table_path,
                format!("operation {id:?} argument contains a control character"),
            ));
        }
    }

    Ok((executable, args.to_vec()))
}

fn invalid_table(table_path: &Path, message: impl Into<String>) -> AppPackageError {
    AppPackageError::new(AppPackageErrorCode::OperationsTableInvalid, message).at(table_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE_PATH: &str = "operations/table.toml";

    const VALID: &str = r#"
schema = "oqto-app-operations/v0-draft"

[[operation]]
id = "comfy.workflows.list"
summary = "List available generation workflows"
exec = ["operations/cmfy-bridge", "workflows-list"]
stdin = "none"
stdout = "lines"
timeout_seconds = 15

[[operation]]
id = "comfy.generate.submit"
summary = "Submit one generation job and return its job id"
exec = ["operations/cmfy-bridge", "generate-submit"]
stdin = "json"
stdout = "json"
timeout_seconds = 60

[operation.params]
workflow = { type = "string", pattern = "^[A-Za-z0-9._-]{1,128}$" }
prompt = { type = "string", max_bytes = 4096 }
"#;

    fn parse(text: &str) -> Result<OperationsTable, AppPackageError> {
        parse_operations_table(text.as_bytes(), 64 * 1024, Path::new(TABLE_PATH))
    }

    #[test]
    fn parses_draft_table_and_preserves_uninterpreted_policy() -> Result<(), AppPackageError> {
        let table = parse(VALID)?;
        assert_eq!(table.schema, OPERATIONS_SCHEMA_V0_DRAFT);
        assert_eq!(table.operations.len(), 2);
        assert_eq!(
            table.operations[0].executable,
            PathBuf::from("operations/cmfy-bridge")
        );
        assert_eq!(table.operations[0].args, vec!["workflows-list".to_owned()]);
        assert_eq!(
            table.operations[1].summary.as_deref(),
            Some("Submit one generation job and return its job id")
        );
        Ok(())
    }

    #[test]
    fn resolves_requested_ids_and_rejects_unknown() -> Result<(), AppPackageError> {
        let table = parse(VALID)?;
        let resolved = table.resolve(&["comfy.generate.submit".to_owned()])?;
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].id, "comfy.generate.submit");

        let error = table
            .resolve(&["comfy.missing".to_owned()])
            .expect_err("unknown id must fail");
        assert_eq!(error.code, AppPackageErrorCode::OperationMissing);
        Ok(())
    }

    #[test]
    fn rejects_unsupported_schema() {
        let input = VALID.replace("oqto-app-operations/v0-draft", "something-else/v9");
        assert_eq!(
            parse(&input).expect_err("schema must fail").code,
            AppPackageErrorCode::OperationsTableInvalid
        );
    }

    #[test]
    fn rejects_unsafe_or_absent_executables() {
        for exec in [
            r#"["cmfy"]"#,
            r#"["/usr/bin/cmfy"]"#,
            r#"["operations/../bundle/index.html"]"#,
            r#"["bundle/index.html"]"#,
            r#"["operations"]"#,
            "[]",
        ] {
            let input = VALID.replace(r#"["operations/cmfy-bridge", "workflows-list"]"#, exec);
            assert_eq!(
                parse(&input).expect_err("exec must fail").code,
                AppPackageErrorCode::OperationsTableInvalid,
                "exec={exec}"
            );
        }
    }

    #[test]
    fn rejects_duplicate_and_malformed_ids() {
        let duplicate = VALID.replace("comfy.generate.submit", "comfy.workflows.list");
        assert_eq!(
            parse(&duplicate).expect_err("duplicate must fail").code,
            AppPackageErrorCode::OperationsTableInvalid
        );

        let malformed = VALID.replace("comfy.workflows.list", "Comfy..List");
        assert_eq!(
            parse(&malformed).expect_err("malformed must fail").code,
            AppPackageErrorCode::OperationsTableInvalid
        );
    }

    #[test]
    fn rejects_oversized_table() {
        let error = parse_operations_table(VALID.as_bytes(), 16, Path::new(TABLE_PATH))
            .expect_err("size must fail");
        assert_eq!(error.code, AppPackageErrorCode::OperationsTableInvalid);
    }
}
