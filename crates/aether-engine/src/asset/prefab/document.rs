use super::migration::PrefabHeader;
use super::{io, migration, validation, PrefabDocument, PrefabError, PREFAB_SCHEMA_VERSION};
use std::collections::BTreeSet;
use std::path::Path;

impl PrefabDocument {
    /// Parse a Prefab RON document and migrate supported older versions.
    pub fn from_ron(source: &str) -> Result<Self, PrefabError> {
        let header: PrefabHeader =
            ron::from_str(source).map_err(|error| PrefabError::Serialization(error.to_string()))?;
        let document = match header.schema_version {
            PREFAB_SCHEMA_VERSION => ron::from_str::<Self>(source)
                .map_err(|error| PrefabError::Serialization(error.to_string()))?,
            1 => migration::migrate_v1(
                ron::from_str::<super::migration::LegacyPrefabDocumentV1>(source)
                    .map_err(|error| PrefabError::Serialization(error.to_string()))?,
            )?,
            version => return Err(PrefabError::SchemaVersion(version)),
        };
        document.validate()?;
        Ok(document)
    }

    /// Validate stable ids, closed component shape, and the mandatory Transform.
    pub fn validate(&self) -> Result<(), PrefabError> {
        if self.schema_version != PREFAB_SCHEMA_VERSION {
            return Err(PrefabError::SchemaVersion(self.schema_version));
        }
        let mut ids = BTreeSet::new();
        validation::validate_node(&self.root, &mut ids)
    }

    /// Serialize the current schema to pretty RON.
    pub fn to_ron(&self) -> Result<String, PrefabError> {
        self.validate()?;
        ron::ser::to_string_pretty(
            self,
            ron::ser::PrettyConfig::new()
                .depth_limit(64)
                .separate_tuple_members(true),
        )
        .map_err(|error| PrefabError::Serialization(error.to_string()))
    }

    /// Write the document to disk through a sibling temporary file.
    pub fn save(&self, path: &Path) -> Result<(), PrefabError> {
        let serialized = self.to_ron()?;
        io::atomic_write(path, serialized.as_bytes())
    }
}
