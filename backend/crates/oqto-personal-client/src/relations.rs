//! Thin adapter only: row admission already belongs to the authenticated Host
//! request. Shared normalization cannot create an endpoint absent from this set.
use crate::Session;
use oqto_session_relations::CatalogRow;

pub(crate) fn normalize(sessions: &mut [Session]) {
    let mut rows: Vec<_> = sessions
        .iter_mut()
        .map(|session| CatalogRow {
            id: session.id.clone(),
            relations: std::mem::take(&mut session.relations),
        })
        .collect();
    oqto_session_relations::normalize(&mut rows);
    for (session, row) in sessions.iter_mut().zip(rows) {
        session.relations = row.relations;
    }
}
