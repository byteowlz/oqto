//! Project only independently validated public relation facts. Untagged legacy
//! lineage (including COALESCE parent IDs) cannot recover a public-ID mapping.
//! Source/root/header/durable-pin admission belongs to Runner, not this pure seam.
use oqto_session_relations::{CatalogRow, MAX_CATALOG_ROWS, Relation, normalize};
use serde_json::Value;
use std::collections::HashMap;

mod reader;
pub(super) use reader::{CatalogContext, Context};

pub(super) async fn enrich_catalog(data: &mut Value, context: Option<CatalogContext<'_>>) {
    let (facts, status, counts) = match context {
        Some(context) => reader::read_catalog(data, context).await,
        None => (Vec::new(), reader::ReadStatus::NoSource, Default::default()),
    };
    project(data, &facts);
    // Only optional additions may be discarded; never trim the base catalog
    // to make an oversized relation projection fit an existing client budget.
    if !catalog_response_fits(data) {
        project(data, &[]);
        log::debug!(
            "Native relation metadata {:?}",
            reader::ReadStatus::ResponseBudget
        );
    } else {
        log::debug!("Native relation metadata {status:?}; counts={counts:?}");
    }
}

fn catalog_response_fits(data: &Value) -> bool {
    struct Bound(usize);
    impl std::io::Write for Bound {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.0 {
                return Err(std::io::ErrorKind::FileTooLarge.into());
            }
            self.0 -= bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Bound(2 * 1024 * 1024), data).is_ok()
}

/// The existing production wrapper deliberately passes None. Only an explicit
/// owner-supplied source and fresh admitted bindings can exercise the reader.
pub(super) async fn enrich(data: &mut Value, context: Option<Context<'_>>) {
    let admitted = match context {
        Some(context) => reader::read(data, context).await,
        None => Vec::new(),
    };
    project(data, &admitted);
}

/// Fields are deliberately private: a producer/source reader must construct this
/// inside this module only after explicit source/identity admission. There is NO
/// production constructor from legacy catalog parent columns. The prepared
/// reader uses the existing durable validator, but the production wrapper still
/// supplies None and all rows stay roots until caller integration is authorized.
pub(super) struct ValidatedRelation {
    child_public: String,
    relation: Relation,
}

pub(super) fn project(data: &mut Value, admitted: &[ValidatedRelation]) {
    let Some(sessions) = data.get_mut("sessions").and_then(Value::as_array_mut) else {
        return;
    };
    let mut by_child: HashMap<&str, Vec<Relation>> = HashMap::new();
    if admitted.len() <= MAX_CATALOG_ROWS * 2 {
        for fact in admitted {
            by_child
                .entry(&fact.child_public)
                .or_default()
                .push(fact.relation.clone());
        }
    }
    let mut projected: Vec<_> = sessions
        .iter()
        .map(|session| {
            let id = session["id"].as_str().unwrap_or_default();
            CatalogRow {
                id: id.to_owned(),
                relations: by_child.remove(id).unwrap_or_default(),
            }
        })
        .collect();
    // Shared validation only restricts this same outgoing admitted source set.
    // It cannot manufacture endpoints or turn legacy fields into evidence.
    normalize(&mut projected);
    for (session, projection) in sessions.iter_mut().zip(projected) {
        if let Some(object) = session.as_object_mut() {
            object.remove("relations");
            if !projection.relations.is_empty()
                && let Ok(value) = serde_json::to_value(projection.relations)
            {
                object.insert("relations".into(), value);
            }
        }
    }
}

#[cfg(test)]
mod tests;
