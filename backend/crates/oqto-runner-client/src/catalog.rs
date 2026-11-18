//! Command catalog state: descriptors as issued, never a client-invented menu.

use crate::contract::{
    RuntimeCatalogCoverage, RuntimeCommandCatalog, RuntimeCommandDescriptor, RuntimeGeneration,
    RuntimeId,
};

/// The catalog a producer last issued, with its own generation.
#[derive(Debug, Default, Clone)]
pub struct CatalogState {
    catalog: Option<RuntimeCommandCatalog>,
}

impl CatalogState {
    #[must_use]
    pub fn is_known(&self) -> bool {
        self.catalog.is_some()
    }

    #[must_use]
    pub fn generation(&self) -> Option<&RuntimeGeneration> {
        self.catalog.as_ref().map(|catalog| &catalog.generation)
    }

    #[must_use]
    pub fn coverage(&self) -> Option<RuntimeCatalogCoverage> {
        self.catalog.as_ref().map(|catalog| catalog.coverage)
    }

    #[must_use]
    pub fn commands(&self) -> &[RuntimeCommandDescriptor] {
        self.catalog
            .as_ref()
            .map_or(&[], |catalog| catalog.commands.as_slice())
    }

    #[must_use]
    pub fn find(&self, command_id: &RuntimeId) -> Option<&RuntimeCommandDescriptor> {
        self.commands()
            .iter()
            .find(|command| &command.command_id == command_id)
    }

    #[must_use]
    pub fn is_available(&self, command_id: &RuntimeId) -> bool {
        self.find(command_id).is_some_and(|command| {
            matches!(
                command.availability,
                crate::contract::RuntimeAvailability::Available
            )
        })
    }

    /// Replace the catalog. Generations are opaque, so a change is accepted and
    /// in-flight dispatch is fenced by the generation check in `dispatch_request`.
    pub fn apply(&mut self, catalog: RuntimeCommandCatalog) {
        self.catalog = Some(catalog);
    }
}
