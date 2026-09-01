//! Deterministic single-document rendering of an immutable App bundle.
//!
//! The entry HTML plus every referenced bundle asset is inlined into one
//! self-contained document delivered over the authenticated API and mounted
//! with `<iframe srcdoc sandbox="allow-scripts...">`. Because no App content
//! URL exists, there is no bearer token to leak and no unauthenticated
//! surface; the opaque sandbox origin isolates the code from OqtoUI.

use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use sha2::{Digest, Sha256};

use super::artifact::AppArtifactStore;
use super::models::{DefinitionAssetRow, StoredBundleFile};

/// Advisory defense in depth inside the already-sandboxed document. The real
/// boundary is the iframe `sandbox` attribute (opaque origin); meta CSP
/// additionally blocks network egress from injected/inlined script.
const META_CSP: &str = "default-src 'none'; script-src 'unsafe-inline'; \
     style-src 'unsafe-inline'; img-src data: blob:; font-src data:; \
     media-src data: blob:; connect-src 'none'; form-action 'none'; base-uri 'none'";

pub async fn render_inlined_document(
    artifacts: &AppArtifactStore,
    definition: &DefinitionAssetRow,
) -> Result<String> {
    let index: Vec<StoredBundleFile> = serde_json::from_str(&definition.file_index_json)
        .context("decoding stored App file index")?;
    let definition_dir = artifacts
        .definition_dir(&definition.content_digest)
        .context("stored App digest is invalid")?;

    let entry_path = validated_relative(Path::new(&definition.web_entry_path))?;
    let entry_bytes = read_verified(&definition_dir, &index, &entry_path).await?;
    let entry_html =
        String::from_utf8(entry_bytes).context("App entry document is not valid UTF-8")?;
    let entry_dir = entry_path
        .parent()
        .context("App entry document has no parent directory")?
        .to_path_buf();

    let mut document = entry_html;

    // Inline <script src="..."></script>
    document = replace_all(
        &document,
        "<script",
        "</script>",
        |attrs| extract_attr(attrs, "src"),
        |reference, _attrs| {
            let asset = resolve_reference(&entry_dir, reference)?;
            Some(InlineAction::Script(asset))
        },
    );

    // Inline <link rel="stylesheet" href="...">
    document = replace_void_tags(&document, "<link", |attrs| {
        if !attrs.to_ascii_lowercase().contains("stylesheet") {
            return None;
        }
        let reference = extract_attr(attrs, "href")?;
        let asset = resolve_reference(&entry_dir, &reference)?;
        Some(InlineAction::Style(asset))
    });

    let mut rendered = String::with_capacity(document.len());
    let mut remaining = document.as_str();
    // Apply queued replacements: markers were embedded as placeholders.
    while let Some(start) = remaining.find('\u{0000}') {
        rendered.push_str(&remaining[..start]);
        let end = remaining[start + 1..]
            .find('\u{0000}')
            .map(|offset| start + 1 + offset)
            .context("unterminated inline marker")?;
        let marker = &remaining[start + 1..end];
        rendered.push_str(&render_marker(&definition_dir, &index, marker).await?);
        remaining = &remaining[end + 1..];
    }
    rendered.push_str(remaining);

    // Rewrite remaining relative references (img/src etc.) to data: URIs.
    let mut final_document = rendered;
    for file in &index {
        let Ok(relative_to_entry) = file_relative_to(&entry_dir, &file.relative_path) else {
            continue;
        };
        let needle_double = format!("\"{relative_to_entry}\"");
        let needle_prefixed = format!("\"./{relative_to_entry}\"");
        if !final_document.contains(&needle_double) && !final_document.contains(&needle_prefixed) {
            continue;
        }
        let bytes = read_verified(&definition_dir, &index, Path::new(&file.relative_path)).await?;
        let mime = mime_guess::from_path(&file.relative_path).first_or_octet_stream();
        let data_uri = format!(
            "data:{};base64,{}",
            mime,
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        );
        let replacement = format!("\"{data_uri}\"");
        final_document = final_document
            .replace(&needle_prefixed, &replacement)
            .replace(&needle_double, &replacement);
    }

    // Inject advisory meta CSP as the first head child.
    let meta = format!("<meta http-equiv=\"Content-Security-Policy\" content=\"{META_CSP}\">");
    if let Some(head_position) = final_document.find("<head>") {
        final_document.insert_str(head_position + "<head>".len(), &meta);
    } else {
        final_document = format!("{meta}{final_document}");
    }

    Ok(final_document)
}

enum InlineAction {
    Script(PathBuf),
    Style(PathBuf),
}

fn replace_all(
    document: &str,
    open_tag: &str,
    close_tag: &str,
    extract: impl Fn(&str) -> Option<String>,
    classify: impl Fn(&str, &str) -> Option<InlineAction>,
) -> String {
    let mut output = String::with_capacity(document.len());
    let mut remaining = document;
    while let Some(start) = find_ci(remaining, open_tag) {
        let Some(tag_end_offset) = remaining[start..].find('>') else {
            break;
        };
        let tag_end = start + tag_end_offset;
        let attrs = &remaining[start + open_tag.len()..tag_end];
        let close = find_ci(&remaining[tag_end..], close_tag)
            .map(|offset| tag_end + offset + close_tag.len());
        let (reference, close_at) = match (extract(attrs), close) {
            (Some(reference), Some(close_at)) => (reference, close_at),
            _ => {
                output.push_str(&remaining[..tag_end + 1]);
                remaining = &remaining[tag_end + 1..];
                continue;
            }
        };
        match classify(&reference, attrs) {
            Some(InlineAction::Script(asset)) => {
                output.push_str(&remaining[..start]);
                output.push_str(&format!(
                    "\u{0000}script:{}\u{0000}",
                    asset.to_string_lossy()
                ));
                remaining = &remaining[close_at..];
            }
            _ => {
                output.push_str(&remaining[..tag_end + 1]);
                remaining = &remaining[tag_end + 1..];
            }
        }
    }
    output.push_str(remaining);
    output
}

fn replace_void_tags(
    document: &str,
    open_tag: &str,
    classify: impl Fn(&str) -> Option<InlineAction>,
) -> String {
    let mut output = String::with_capacity(document.len());
    let mut remaining = document;
    while let Some(start) = find_ci(remaining, open_tag) {
        let Some(tag_end_offset) = remaining[start..].find('>') else {
            break;
        };
        let tag_end = start + tag_end_offset;
        let attrs = &remaining[start + open_tag.len()..tag_end];
        match classify(attrs) {
            Some(InlineAction::Style(asset)) => {
                output.push_str(&remaining[..start]);
                output.push_str(&format!(
                    "\u{0000}style:{}\u{0000}",
                    asset.to_string_lossy()
                ));
                remaining = &remaining[tag_end + 1..];
            }
            _ => {
                output.push_str(&remaining[..tag_end + 1]);
                remaining = &remaining[tag_end + 1..];
            }
        }
    }
    output.push_str(remaining);
    output
}

async fn render_marker(
    definition_dir: &Path,
    index: &[StoredBundleFile],
    marker: &str,
) -> Result<String> {
    let (kind, path) = marker.split_once(':').context("malformed inline marker")?;
    let bytes = read_verified(definition_dir, index, Path::new(path)).await?;
    let text = String::from_utf8(bytes).context("inlined App asset is not valid UTF-8")?;
    match kind {
        "script" => Ok(format!(
            "<script>{}</script>",
            text.replace("</script", "<\\/script")
        )),
        "style" => Ok(format!("<style>{text}</style>")),
        other => bail!("unknown inline marker kind {other:?}"),
    }
}

async fn read_verified(
    definition_dir: &Path,
    index: &[StoredBundleFile],
    relative_path: &Path,
) -> Result<Vec<u8>> {
    let portable = relative_path.to_string_lossy().replace('\\', "/");
    let expected = index
        .iter()
        .find(|entry| entry.relative_path == portable)
        .with_context(|| format!("App asset {portable} is not in the immutable file index"))?;
    let bytes = tokio::fs::read(definition_dir.join(relative_path))
        .await
        .with_context(|| format!("reading immutable App asset {portable}"))?;
    let actual = hex::encode(Sha256::digest(&bytes));
    if actual != expected.sha256 {
        bail!("immutable App asset integrity failure for {portable}");
    }
    Ok(bytes)
}

fn resolve_reference(entry_dir: &Path, reference: &str) -> Option<PathBuf> {
    if reference.is_empty()
        || reference.contains(':')
        || reference.contains('\\')
        || reference.starts_with('/')
        || reference.starts_with('#')
    {
        return None;
    }
    let trimmed = reference.strip_prefix("./").unwrap_or(reference);
    validated_relative(&entry_dir.join(trimmed)).ok()
}

fn file_relative_to(entry_dir: &Path, portable_path: &str) -> Result<String> {
    let path = Path::new(portable_path);
    let relative = path
        .strip_prefix(entry_dir)
        .context("asset not beneath entry directory")?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn validated_relative(path: &Path) -> Result<PathBuf> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("invalid App bundle relative path");
    }
    Ok(path.to_path_buf())
}

fn extract_attr(attrs: &str, name: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();
    let position = lower.find(&format!("{name}="))?;
    let after = &attrs[position + name.len() + 1..];
    let quote = after.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &after[1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_owned())
}

fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .to_ascii_lowercase()
        .find(&needle.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_resolution_rejects_escapes_and_urls() {
        let entry_dir = Path::new("bundle");
        assert!(resolve_reference(entry_dir, "./assets/app.js").is_some());
        assert!(resolve_reference(entry_dir, "assets/app.css").is_some());
        assert!(resolve_reference(entry_dir, "../oqto-app.toml").is_none());
        assert!(resolve_reference(entry_dir, "https://evil.example/x.js").is_none());
        assert!(resolve_reference(entry_dir, "/etc/passwd").is_none());
        assert!(resolve_reference(entry_dir, "#fragment").is_none());
    }

    #[test]
    fn script_close_tag_is_neutralized() {
        let escaped = "</script><script>alert(1)".replace("</script", "<\\/script");
        assert!(!escaped.contains("</script>"));
    }
}
