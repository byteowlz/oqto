//! Bounded read-only provenance inventory, NOT endpoint admission authority.
//! Every selected endpoint must still pass the existing `validate` operation.
use super::reader::ReadShard;
use anyhow::{Result, ensure};
use futures::TryStreamExt;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::path::{Path, PathBuf};

// Every current shard is opened one at a time and the row/byte budgets below cap
// what is retained, so this only bounds total work. It must still exceed a real
// catalog: a working machine reached 264 Workspaces on 2026-11-17, and at 256
// every relation read failed closed ("Native provenance shard budget exceeded"),
// which silently disabled Session nesting on the phone for the whole catalog.
// Keep it above the shard count a busy machine reaches, and below the directory
// entry budget so the entry bound stays the tighter one.
const SHARDS: usize = 2048;
const ENTRIES: usize = 4096;
const ROWS: usize = 65536;
const BYTES: usize = 16 * 1024 * 1024;
const PIN_BYTES: usize = 16384;
const ALL_PIN_BYTES: usize = 4 * 1024 * 1024;
const RAW_ENVELOPE_BYTES: usize = 64 * 1024;
const ALL_RAW_ENVELOPE_BYTES: usize = 16 * 1024 * 1024;

#[derive(PartialEq, Eq)]
pub struct CatalogPin {
    pub public: String,
    pub external: String,
    pub workspace: String,
    pub pin: String,
}
#[derive(PartialEq, Eq)]
pub struct CatalogPins {
    pub pins: Vec<CatalogPin>,
    /// Identity and binding facts only; never a transcript/title fingerprint.
    fingerprint: [u8; 32],
    pub shards: usize,
    pub identity_rows: usize,
    pub binding_rows: usize,
    pub bytes: usize,
    pub raw_envelope_bytes: usize,
}

async fn paths(home: &Path) -> Result<Vec<PathBuf>> {
    let directory = home.join(".local/share/oqto/oqto-log");
    let permit = super::INVENTORY_SCANNER.acquire().await?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut paths = Vec::new();
        for (index, entry) in entries.enumerate() {
            ensure!(index < ENTRIES, "Native provenance entry budget exceeded");
            let entry = entry?;
            let kind = entry.file_type()?;
            if !kind.is_dir() && !kind.is_symlink() {
                continue;
            }
            if !std::fs::metadata(entry.path())?.is_dir() {
                continue;
            }
            let db = entry.path().join("oqto-log.sqlite");
            ensure!(
                std::fs::metadata(&db)?.is_file(),
                "Incomplete native provenance inventory"
            );
            ensure!(
                paths.len() < SHARDS,
                "Native provenance shard budget exceeded"
            );
            paths.push(db);
        }
        paths.sort();
        Ok(paths)
    })
    .await?
}

/// Complete current canonical identity/tag inventory under an explicit HOME.
/// Missing extensions uses the existing validator's NULL-source semantics.
/// Missing core identity schema/incomplete shards fail closed, never become a
/// guessed nonnative partition. Values are length-bounded before decoding.
pub async fn catalog_pins(home: &Path) -> Result<CatalogPins> {
    let mut hash = Sha256::new();
    let paths = paths(home).await?;
    let mut result = CatalogPins {
        pins: Vec::new(),
        fingerprint: [0; 32],
        shards: paths.len(),
        identity_rows: 0,
        binding_rows: 0,
        bytes: 0,
        raw_envelope_bytes: 0,
    };
    let mut pin_bytes = 0;
    for path in paths {
        hash.update(b"shard");
        hash.update((path.as_os_str().as_encoded_bytes().len() as u64).to_be_bytes());
        hash.update(path.as_os_str().as_encoded_bytes());
        let mut shard = ReadShard::open(&path).await?;
        let columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('oqto_log_sessions')")
                .fetch_all(shard.connection())
                .await?;
        ensure!(
            ["session_id", "platform_id", "external_id", "workspace_id"]
                .iter()
                .all(|name| columns.iter().any(|column| column == name)),
            "Incomplete native provenance schema"
        );
        let has_extensions = columns.iter().any(|name| name == "extensions_json");
        let raw_length = if has_extensions {
            "COALESCE(length(CAST(extensions_json AS BLOB)),0)"
        } else {
            "0"
        };
        let bad_envelope = if has_extensions {
            "typeof(extensions_json) NOT IN ('text','null') OR COALESCE(length(CAST(extensions_json AS BLOB)),0)>65536"
        } else {
            "0"
        };
        // Preflight lengths/types only: NEVER parse an unbounded raw envelope.
        // LIMIT counts every identity row, including nonnative/unknown rows.
        let preflight = format!(
            "SELECT COUNT(*) AS rows,COALESCE(SUM(raw_length),0) AS bytes,COALESCE(MAX(raw_length),0) AS largest,COALESCE(MAX(bad),0) AS bad FROM (SELECT {raw_length} AS raw_length,({bad_envelope}) AS bad FROM oqto_log_sessions LIMIT ?)"
        );
        let budget = ROWS.saturating_sub(result.identity_rows + result.binding_rows);
        let lengths = sqlx::query(&preflight)
            .bind((budget + 1) as i64)
            .fetch_one(shard.connection())
            .await?;
        let row_count: i64 = lengths.try_get("rows")?;
        let raw_bytes: i64 = lengths.try_get("bytes")?;
        ensure!(
            row_count >= 0 && row_count as usize <= budget,
            "Native provenance row budget exceeded"
        );
        ensure!(
            lengths.try_get::<i64, _>("bad")? == 0
                && lengths.try_get::<i64, _>("largest")? <= RAW_ENVELOPE_BYTES as i64
                && raw_bytes >= 0
                && raw_bytes as usize
                    <= ALL_RAW_ENVELOPE_BYTES.saturating_sub(result.raw_envelope_bytes),
            "Native raw envelope budget or type unavailable"
        );
        let source = if has_extensions {
            // CASE is lazy in SQLite. Repeat the guard for writes between
            // preflight/read; oversized or unknown is flagged, NEVER no-pin.
            "CASE WHEN typeof(extensions_json)='null' THEN NULL WHEN typeof(extensions_json)='text' AND length(CAST(extensions_json AS BLOB))<=65536 THEN CASE WHEN json_valid(extensions_json) THEN CASE WHEN json_type(extensions_json)='object' THEN json_extract(extensions_json,'$.personal_native_source') ELSE NULL END ELSE NULL END ELSE NULL END"
        } else {
            "NULL"
        };
        let bad_json = if has_extensions {
            "CASE WHEN typeof(extensions_json)='null' THEN 0 WHEN typeof(extensions_json)='text' AND length(CAST(extensions_json AS BLOB))<=65536 THEN CASE WHEN json_valid(extensions_json) THEN json_type(extensions_json)!='object' ELSE 1 END ELSE 1 END"
        } else {
            "0"
        };
        let query = format!(
            "SELECT substr(session_id,1,257) AS raw,substr(platform_id,1,257) AS public,substr(external_id,1,257) AS external,substr(workspace_id,1,4097) AS workspace,substr({source},1,16385) AS pin,{raw_length} AS raw_length,({bad_json}) AS bad_envelope FROM oqto_log_sessions ORDER BY session_id LIMIT ?"
        );
        {
            let mut rows = sqlx::query(&query)
                .bind((ROWS.saturating_sub(result.identity_rows + result.binding_rows) + 1) as i64)
                .fetch(shard.connection());
            while let Some(row) = rows.try_next().await? {
                ensure!(
                    result.identity_rows + result.binding_rows < ROWS,
                    "Native provenance row budget exceeded"
                );
                result.identity_rows += 1;
                let envelope_bytes: i64 = row.try_get("raw_length")?;
                ensure!(
                    row.try_get::<i64, _>("bad_envelope")? == 0
                        && envelope_bytes >= 0
                        && envelope_bytes as usize <= RAW_ENVELOPE_BYTES
                        && envelope_bytes as usize
                            <= ALL_RAW_ENVELOPE_BYTES.saturating_sub(result.raw_envelope_bytes),
                    "Native raw envelope budget or type unavailable"
                );
                result.raw_envelope_bytes += envelope_bytes as usize;
                let raw: String = row.try_get("raw")?;
                let public: Option<String> = row.try_get("public")?;
                let external: Option<String> = row.try_get("external")?;
                let workspace: Option<String> = row.try_get("workspace")?;
                let pin: Option<String> = row.try_get("pin")?;
                ensure!(
                    raw.len() <= 256
                        && public.as_ref().is_none_or(|value| value.len() <= 256)
                        && external.as_ref().is_none_or(|value| value.len() <= 256)
                        && workspace.as_ref().is_none_or(|value| value.len() <= 4096)
                        && pin.as_ref().is_none_or(|value| value.len() <= PIN_BYTES),
                    "Native provenance value budget exceeded"
                );
                hash.update(b"identity");
                for value in [
                    Some(raw.as_str()),
                    public.as_deref(),
                    external.as_deref(),
                    workspace.as_deref(),
                    pin.as_deref(),
                ] {
                    hash.update([u8::from(value.is_some())]);
                    let bytes = value.unwrap_or_default().as_bytes();
                    result.bytes += bytes.len();
                    ensure!(
                        result.bytes <= BYTES,
                        "Native provenance byte budget exceeded"
                    );
                    hash.update((bytes.len() as u64).to_be_bytes());
                    hash.update(bytes);
                }
                if let Some(pin) = pin {
                    pin_bytes += pin.len();
                    ensure!(
                        pin_bytes <= ALL_PIN_BYTES,
                        "Native provenance pin budget exceeded"
                    );
                    result.pins.push(CatalogPin {
                        public: public.ok_or_else(|| {
                            anyhow::anyhow!("Incomplete native provenance identity")
                        })?,
                        external: external.ok_or_else(|| {
                            anyhow::anyhow!("Incomplete native provenance identity")
                        })?,
                        workspace: workspace.ok_or_else(|| {
                            anyhow::anyhow!("Incomplete native provenance identity")
                        })?,
                        pin,
                    });
                }
            }
        }
        let has_bindings: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM native_identity.sqlite_master WHERE type='table' AND name='oqto_log_session_bindings')").fetch_one(shard.connection()).await?;
        hash.update([u8::from(has_bindings)]);
        if has_bindings {
            // Include authority/supersession facts in the snapshot fence without
            // treating them as new admission rules or exposing them to clients.
            let mut rows = sqlx::query("SELECT CASE WHEN max(length(CAST(binding_id AS BLOB)),length(CAST(oqto_session_id AS BLOB)),length(CAST(harness AS BLOB)),length(CAST(provider_instance AS BLOB)),length(CAST(binding_kind AS BLOB)),length(CAST(external_id AS BLOB)),length(CAST(source AS BLOB)),COALESCE(length(CAST(supersedes_binding_id AS BLOB)),0))<=256 THEN json_array(binding_id,oqto_session_id,harness,provider_instance,binding_kind,external_id,source,supersedes_binding_id) ELSE NULL END AS fact FROM oqto_log_session_bindings ORDER BY binding_id LIMIT ?")
                .bind((ROWS.saturating_sub(result.identity_rows + result.binding_rows) + 1) as i64).fetch(shard.connection());
            while let Some(row) = rows.try_next().await? {
                ensure!(
                    result.identity_rows + result.binding_rows < ROWS,
                    "Native provenance row budget exceeded"
                );
                result.binding_rows += 1;
                let fact: String = row.try_get("fact")?;
                ensure!(
                    fact.len() <= PIN_BYTES,
                    "Native provenance binding budget exceeded"
                );
                result.bytes += fact.len();
                ensure!(
                    result.bytes <= BYTES,
                    "Native provenance byte budget exceeded"
                );
                hash.update(b"binding");
                hash.update((fact.len() as u64).to_be_bytes());
                hash.update(fact.as_bytes());
            }
        }
        shard.finish().await?;
    }
    result.pins.sort_by(|left, right| {
        (&left.public, &left.external, &left.workspace, &left.pin).cmp(&(
            &right.public,
            &right.external,
            &right.workspace,
            &right.pin,
        ))
    });
    result.fingerprint = hash.finalize().into();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn home_with_shards(count: usize) -> Result<tempfile::TempDir> {
        let dir = tempfile::tempdir()?;
        let log = dir
            .path()
            .canonicalize()?
            .join(".local/share/oqto/oqto-log");
        std::fs::create_dir_all(&log)?;
        for index in 0..count {
            let shard = log.join(format!("workspace{index:04}"));
            std::fs::create_dir(&shard)?;
            std::fs::write(shard.join("oqto-log.sqlite"), b"")?;
        }
        Ok(dir)
    }

    /// Owner report 2026-11-17: a working machine reached 264 Workspaces, the shard
    /// bound sat at 256, and every relation read failed closed, so no Session nested.
    #[tokio::test]
    async fn inventory_covers_a_busy_machines_shard_count() -> Result<()> {
        let dir = home_with_shards(300).await?;
        let found = paths(&dir.path().canonicalize()?).await?;
        assert_eq!(found.len(), 300);
        Ok(())
    }

    #[tokio::test]
    async fn a_shard_directory_without_its_database_still_fails_closed() -> Result<()> {
        let dir = home_with_shards(1).await?;
        let home = dir.path().canonicalize()?;
        let log = home.join(".local/share/oqto/oqto-log");
        // A store path that is not a database file: incomplete, never a partial list.
        let half = log.join("half-written");
        std::fs::create_dir(&half)?;
        std::fs::create_dir(half.join("oqto-log.sqlite"))?;
        let error = paths(&home).await.unwrap_err();
        let text = format!("{error:#}");
        assert!(
            text.contains("Incomplete native provenance inventory"),
            "{text}"
        );
        Ok(())
    }
}
