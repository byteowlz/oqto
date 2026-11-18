//! Export a portable DTO bundle into an empty directory. Never enables runtime routing.
use oqto_protocol::runtime_interactions::{
    RUNTIME_SOURCE_CAPABILITY, RuntimeEvent, RuntimeRequest, RuntimeResponse, RuntimeSourceFrame,
    RuntimeSourceRequest, RuntimeSourceResponse, decode_runtime_request,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, error::Error, fs, path::Path};
use ts_rs::TS;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}

fn file_hash(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}

fn export_schemas(output: &Path) -> Result<()> {
    for (name, schema) in [
        ("request.schema.json", schemars::schema_for!(RuntimeRequest)),
        (
            "response.schema.json",
            schemars::schema_for!(RuntimeResponse),
        ),
        ("event.schema.json", schemars::schema_for!(RuntimeEvent)),
        (
            "source-request.schema.json",
            schemars::schema_for!(RuntimeSourceRequest),
        ),
        (
            "source-response.schema.json",
            schemars::schema_for!(RuntimeSourceResponse),
        ),
        (
            "source-frame.schema.json",
            schemars::schema_for!(RuntimeSourceFrame),
        ),
    ] {
        write_json(&output.join(name), &schema)?;
    }
    Ok(())
}

fn export_operations(output: &Path, fixtures: &Path) -> Result<()> {
    let corpus: serde_json::Value = serde_json::from_slice(&fs::read(fixtures)?)?;
    let mut operations = BTreeMap::new();
    for case in corpus["requests"].as_array().ok_or("missing requests")? {
        let request = decode_runtime_request(&serde_json::to_vec(&case["value"])?)?;
        let op = case["value"]["operation"]["op"]
            .as_str()
            .ok_or("missing operation")?;
        operations.insert(
            op,
            serde_json::json!({
                "cliPath":request.operation.cli_path(),
                "implementation":"contract_only",
                "requiredAdapters":["personal_tui","managed_rpc","cli_api"]
            }),
        );
    }
    write_json(&output.join("operations.json"), &operations)
}

fn export_type_checks(output: &Path, fixtures: &Path) -> Result<()> {
    let corpus: serde_json::Value = serde_json::from_slice(&fs::read(fixtures)?)?;
    let mut source =
        String::from("// Generated static shape checks; schema/serde enforce runtime bounds.\n");
    for (frame, ty) in [
        ("request", "RuntimeRequest"),
        ("response", "RuntimeResponse"),
        ("event", "RuntimeEvent"),
    ] {
        source.push_str(&format!("import type {{ {ty} }} from './{ty}';\n"));
        let values = corpus[format!("{frame}s")]
            .as_array()
            .ok_or("missing corpus family")?
            .iter()
            .map(|case| &case["value"])
            .collect::<Vec<_>>();
        source.push_str(&format!(
            "export const {frame}s = {} satisfies {ty}[];\n",
            serde_json::to_string_pretty(&values)?
        ));
    }
    for (index, case) in corpus["invalid"]
        .as_array()
        .ok_or("missing invalid cases")?
        .iter()
        .enumerate()
    {
        if [
            "version_2",
            "forged_authority",
            "literal_prompt_not_operation",
            "missing_catalog_generation",
            "answer_cannot_change_model",
        ]
        .contains(&case["name"].as_str().ok_or("missing case name")?)
        {
            source.push_str(&format!("// @ts-expect-error Invalid canonical request shape.\nconst invalid{index}: RuntimeRequest = {};\n", serde_json::to_string(&case["value"])?));
        }
    }
    fs::write(output.join("fixture-types.ts"), source)?;
    Ok(())
}

fn export_source_checks(output: &Path, fixtures: &Path) -> Result<()> {
    let corpus: serde_json::Value = serde_json::from_slice(&fs::read(fixtures)?)?;
    let values = corpus["frames"]
        .as_array()
        .ok_or("missing source frames")?
        .iter()
        .map(|case| &case["value"])
        .collect::<Vec<_>>();
    let source = format!(
        "// Generated source frame shape checks; not authenticated host admission.\nimport type {{ RuntimeSourceFrame }} from './RuntimeSourceFrame';\nexport const frames = {} satisfies RuntimeSourceFrame[];\n",
        serde_json::to_string_pretty(&values)?
    );
    let mut source = source;
    for (index, case) in corpus["invalid"]
        .as_array()
        .ok_or("missing source invalid cases")?
        .iter()
        .take(3)
        .enumerate()
    {
        source.push_str(&format!("// @ts-expect-error Host claims cannot enter native source frames.\nconst invalidSource{index}: RuntimeSourceFrame = {};\n", serde_json::to_string(&case["value"])?));
    }
    fs::write(output.join("source-fixture-types.ts"), source)?;
    Ok(())
}

fn export_index(output: &Path) -> Result<()> {
    let mut names = Vec::new();
    for entry in fs::read_dir(output)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "ts") {
            names.push(
                path.file_stem()
                    .ok_or("missing type name")?
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    names.sort();
    let index = names
        .iter()
        .map(|name| format!("export type {{ {name} }} from './{name}';\n"))
        .collect::<String>();
    fs::write(output.join("index.ts"), index)?;
    Ok(())
}

fn collect_hashes(
    directory: &Path,
    root: &Path,
    files: &mut BTreeMap<String, String>,
) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_hashes(&path, root, files)?;
        } else {
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            files.insert(relative, file_hash(&path)?);
        }
    }
    Ok(())
}

fn export_manifest(output: &Path, root: &Path) -> Result<()> {
    let mut files = BTreeMap::new();
    collect_hashes(output, output, &mut files)?;
    let mut inputs = BTreeMap::new();
    for path in [
        "backend/crates/oqto-protocol/src/runtime_interactions.rs",
        "backend/crates/oqto-protocol/src/runtime_interactions/fixtures.json",
        "backend/crates/oqto-protocol/src/runtime_interactions/source-fixtures.json",
        "backend/crates/oqto-protocol/examples/export_runtime_interactions.rs",
        "backend/crates/oqto-protocol/Cargo.toml",
        // The workspace lockfile is deliberately NOT an input: it churns on every
        // unrelated dependency, which would invalidate consumer pins without any
        // contract change. The generated bytes and this crate's manifest already
        // pin what the bundle was made from.
        "LICENSE",
    ] {
        inputs.insert(path, file_hash(&root.join(path))?);
    }
    write_json(
        &output.join("manifest.json"),
        &serde_json::json!({
            "contract":"oqto.runtime-interactions", "version":1, "normative":true,
            "scope":"DTO/codec conformance; no routing, engine or authority grant",
            "inputs":inputs, "files":files
        }),
    )
}

fn main() -> Result<()> {
    let output = std::env::args_os()
        .nth(1)
        .ok_or("usage: export_runtime_interactions EMPTY_OUTPUT_DIRECTORY")?;
    let output = Path::new(&output);
    fs::create_dir_all(output)?;
    if fs::read_dir(output)?.next().is_some() {
        return Err("output must be empty; generate separately and review the diff".into());
    }
    let package = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = package
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or("missing repository root")?;
    RuntimeRequest::export_all_to(output)?;
    RuntimeResponse::export_all_to(output)?;
    RuntimeEvent::export_all_to(output)?;
    RuntimeSourceFrame::export_all_to(output)?;
    export_schemas(output)?;
    write_json(
        &output.join("transport.json"),
        &serde_json::json!({
            "sourceCapability": RUNTIME_SOURCE_CAPABILITY,
            "version": 1,
            "negotiate": {"type":"interaction_negotiate","versions":[1]},
            "negotiatedType":"interaction_negotiated",
            "continuityField":"interactionRuntimeGeneration",
            "hostBindingOnNativeWire": false,
            "sourceResultIsHostAdmission": false,
            "automaticReplay": false
        }),
    )?;
    let fixtures = package.join("src/runtime_interactions/fixtures.json");
    fs::copy(&fixtures, output.join("fixtures.json"))?;
    let source_fixtures = package.join("src/runtime_interactions/source-fixtures.json");
    fs::copy(&source_fixtures, output.join("source-fixtures.json"))?;
    fs::copy(root.join("LICENSE"), output.join("LICENSE"))?;
    export_operations(output, &fixtures)?;
    export_index(output)?;
    export_type_checks(output, &fixtures)?;
    export_source_checks(output, &source_fixtures)?;
    export_manifest(output, root)
}
