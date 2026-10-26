//! Read-only native-file index: metadata for the tree, payloads for one page.
//! No import, head selection, identity bootstrap or file writes. The caller owns
//! source authorization and must fence file identity/metadata around each read.
use super::*;
use crate::oqto_log::projector::{NATIVE_CURSOR, project_native_branch_page};
use oqto_protocol::projection::ProjectedChatMessagePage;
use serde::Deserialize;
use serde_json::value::RawValue;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;
const PAGE_BYTES: usize = 8 * 1024 * 1024;
const SCAN_TIME: Duration = Duration::from_secs(10);

struct IndexedEntry {
    id: Arc<str>,
    offset: u64,
    length: usize,
    digest: [u8; 32],
    parent_message: Option<usize>,
    nearest_message: Option<usize>,
    nearest_visible: Option<usize>,
    parent_visible: Option<usize>,
    visible_count: usize,
}

/// Small, bounded entry/offset metadata, independent of a particular live head.
/// Reuse only for the exact unchanged source snapshot; never infer append safety
/// from file growth. Payload memory is bounded by one record/page, not file size.
pub struct NativeFileIndex {
    entries: Vec<IndexedEntry>,
    ids: HashMap<Arc<str>, usize>,
    unwritten_parents: std::collections::HashSet<String>,
}

#[derive(Deserialize)]
struct Envelope<'a> {
    #[serde(rename = "type")]
    kind: String,
    id: String,
    #[serde(rename = "parentId")]
    parent: Parent,
    #[serde(borrow)]
    message: Option<&'a RawValue>,
}
enum Parent {
    Root,
    Id(String),
}
impl<'de> Deserialize<'de> for Parent {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct ParentVisitor;
        impl serde::de::Visitor<'_> for ParentVisitor {
            type Value = Parent;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an explicit null or native parent ID")
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Parent, E> {
                Ok(Parent::Root)
            }
            fn visit_str<E: serde::de::Error>(self, id: &str) -> std::result::Result<Parent, E> {
                Ok(Parent::Id(id.to_owned()))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                id: String,
            ) -> std::result::Result<Parent, E> {
                Ok(Parent::Id(id))
            }
        }
        // Do not deserialize an arbitrary Value: a small malicious parent array
        // must not amplify into a large allocation. Missing is not explicit null.
        deserializer.deserialize_any(ParentVisitor)
    }
}

#[derive(Deserialize)]
struct MessageMetadata {
    role: String,
    #[serde(rename = "stopReason")]
    stop_reason: Option<String>,
    // Validate AgentMessage's known metadata without decoding content/extra.
    // Hidden/off-branch records cannot conceal invalid typed metadata.
    #[serde(rename = "timestamp")]
    _timestamp: Option<u64>,
    #[serde(rename = "toolCallId")]
    _tool_call_id: Option<String>,
    #[serde(rename = "toolName")]
    _tool_name: Option<String>,
    #[serde(rename = "isError")]
    _is_error: Option<bool>,
    #[serde(rename = "api")]
    _api: Option<String>,
    #[serde(rename = "provider")]
    _provider: Option<String>,
    #[serde(rename = "model")]
    _model: Option<String>,
    #[serde(rename = "usage")]
    _usage: Option<oqto_pi::TokenUsage>,
}

impl NativeFileIndex {
    /// Scan a captured file length, validating complete record IDs/parent links.
    /// Unselected payloads are syntax-checked but never materialized as Values.
    /// An unfinished final record is not durable and cannot supply the head.
    pub fn read<R: Read + Seek>(
        file: &mut R,
        length: u64,
        native_id: &str,
        workspace: &str,
    ) -> Result<Self> {
        ensure!(length <= MAX_FILE_BYTES, "Native index file bound exceeded");
        token(native_id)?;
        file.seek(SeekFrom::Start(0))?;
        let mut reader = BufReader::new(file.take(length));
        let header = bounded_line(&mut reader, 16 * 1024)?;
        ensure!(header.ends_with(b"\n"), "Incomplete native source header");
        let value: Value = serde_json::from_slice(&header)?;
        ensure!(
            value["type"] == "session" && value["id"] == native_id && value["cwd"] == workspace,
            "Native source identity mismatch"
        );
        let mut index = Self {
            entries: Vec::new(),
            ids: HashMap::new(),
            unwritten_parents: Default::default(),
        };
        let deadline = Instant::now() + SCAN_TIME;
        let mut offset = header.len() as u64;
        loop {
            ensure!(
                Instant::now() < deadline,
                "Native index scan deadline exceeded"
            );
            let line = bounded_line(&mut reader, MAX_RECORD_BYTES)?;
            if !line.ends_with(b"\n") {
                break;
            }
            index.record(offset, &line)?;
            offset += line.len() as u64;
        }
        Ok(index)
    }

    fn record(&mut self, offset: u64, line: &[u8]) -> Result<()> {
        ensure!(
            self.entries.len() < MAX_ENTRIES,
            "Native entry bound exceeded"
        );
        let raw: Envelope<'_> = serde_json::from_str(std::str::from_utf8(line)?)?;
        token(&raw.id)?;
        ensure!(
            !self.ids.contains_key(raw.id.as_str()),
            "Duplicate native entry identity"
        );
        // Pi starts a branch at an entry whose parent was never written. A parent
        // written later (or the entry itself) is a link Pi would follow: refuse it.
        let parent = match &raw.parent {
            Parent::Root => None,
            Parent::Id(id) => {
                token(id)?;
                let parent = self.ids.get(id.as_str()).copied();
                if parent.is_none() {
                    self.unwritten_parents.insert(id.clone());
                }
                parent
            }
        };
        ensure!(
            !self.unwritten_parents.contains(&raw.id),
            "Native parent absent or cyclic"
        );
        let parent_message = parent.and_then(|i| self.entries[i].nearest_message);
        let parent_visible = parent.and_then(|i| self.entries[i].nearest_visible);
        let message = raw.kind == "message";
        let visible = if message {
            let data: MessageMetadata =
                serde_json::from_str(raw.message.context("Native message missing")?.get())?;
            normalize_role(&data.role) != "assistant"
                || (data.role != "bashExecution" && data.stop_reason.as_deref() != Some("aborted"))
        } else {
            false
        };
        let n = self.entries.len();
        let id: Arc<str> = raw.id.into();
        let entry = IndexedEntry {
            id: id.clone(),
            offset,
            length: line.len(),
            digest: Sha256::digest(line).into(),
            parent_message,
            nearest_message: if message { Some(n) } else { parent_message },
            nearest_visible: if visible { Some(n) } else { parent_visible },
            parent_visible,
            visible_count: parent.map_or(0, |i| self.entries[i].visible_count)
                + usize::from(visible),
        };
        self.ids.insert(id, n);
        self.entries.push(entry);
        Ok(())
    }

    /// The selected live branch only. Existing native cursors count visible
    /// ancestors from the root, so paging remains stable across branch growth.
    /// Large records reduce page length rather than dropping visible messages.
    pub fn page<R: Read + Seek>(
        &self,
        file: &mut R,
        public_id: &str,
        leaf: Option<&str>,
        limit: usize,
        before: Option<&str>,
    ) -> Result<ProjectedChatMessagePage> {
        let (mut next, end) = self.window(public_id, leaf, before)?;
        let mut start = end;
        let mut bytes = 0;
        let mut branch = Vec::new();
        while let Some(i) = next {
            let entry = &self.entries[i];
            next = entry.parent_visible;
            if branch.len() >= limit.clamp(1, 200) {
                break;
            }
            ensure!(
                entry.length <= PAGE_BYTES,
                "Native message exceeds page byte bound"
            );
            if bytes + entry.length > PAGE_BYTES {
                break;
            }
            bytes += entry.length;
            branch.push(self.message(file, i, public_id)?);
            start = entry.visible_count - 1;
        }
        branch.reverse();
        let mut page = project_native_branch_page(public_id, &branch, branch.len().max(1), None)?;
        ensure!(
            page.messages.len() == branch.len(),
            "Native visibility changed during page read"
        );
        page.has_more = start > 0;
        page.next_before = if page.has_more {
            Some(format!(
                "{NATIVE_CURSOR}v2:{start}:{}",
                branch
                    .first()
                    .context("Native cursor anchor missing")?
                    .message_id
            ))
        } else {
            None
        };
        Ok(page)
    }

    /// Bind the cursor to a public message that must still be an ancestor of
    /// the selected head. Growth is safe; branch switches cannot mix pages.
    /// Old numeric cursors must refresh instead of silently losing that fence.
    fn window(
        &self,
        public: &str,
        leaf: Option<&str>,
        before: Option<&str>,
    ) -> Result<(Option<usize>, usize)> {
        let head = match leaf {
            None => None,
            Some(id) => {
                token(id)?;
                Some(
                    *self
                        .ids
                        .get(id)
                        .context("Native head is not durably present")?,
                )
            }
        };
        let count = head.map_or(0, |i| self.entries[i].visible_count);
        let (end, anchor) = match before {
            None => (count, None),
            Some(cursor) => {
                let (end, anchor) = cursor
                    .strip_prefix(NATIVE_CURSOR)
                    .and_then(|s| s.strip_prefix("v2:"))
                    .and_then(|s| s.split_once(':'))
                    .context("Native cursor requires a fresh newest page")?;
                let end = end.parse::<usize>().context("Invalid native page cursor")?;
                ensure!(end <= count, "Native cursor outside branch");
                (end, Some(anchor))
            }
        };
        let mut next = head.and_then(|i| self.entries[i].nearest_visible);
        let mut matched = anchor.is_none();
        while let Some(i) = next {
            let entry = &self.entries[i];
            if entry.visible_count <= end {
                break;
            }
            if entry.visible_count == end + 1
                && anchor == Some(stable_id("message", public, &entry.id).as_str())
            {
                matched = true;
            }
            next = entry.parent_visible;
        }
        ensure!(
            matched,
            "Native cursor no longer belongs to selected branch"
        );
        Ok((next, end))
    }

    fn message<R: Read + Seek>(
        &self,
        file: &mut R,
        i: usize,
        public_id: &str,
    ) -> Result<crate::oqto_log::projector::NativeBranchMessage> {
        let entry = &self.entries[i];
        file.seek(SeekFrom::Start(entry.offset))?;
        let mut bytes = vec![0; entry.length];
        file.read_exact(&mut bytes)?;
        ensure!(
            <[u8; 32]>::from(Sha256::digest(&bytes)) == entry.digest,
            "Native indexed record changed"
        );
        let raw: Value = serde_json::from_slice(&bytes)?;
        let message = serde_json::from_value(raw["message"].clone())?;
        let value = Entry {
            id: entry.id.to_string(),
            raw,
            message: Some(message),
            parent_message: entry.parent_message.map(|i| self.entries[i].id.to_string()),
        };
        value
            .branch_message(public_id)
            .context("Native message projection unavailable")
    }
}

fn bounded_line<R: BufRead>(reader: &mut R, bound: usize) -> Result<Vec<u8>> {
    let mut line = Vec::new();
    reader.take(bound as u64 + 1).read_until(b'\n', &mut line)?;
    ensure!(line.len() <= bound, "Native record bound exceeded");
    Ok(line)
}

#[cfg(test)]
mod tests;
