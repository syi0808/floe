//! One fixed physical layout per database. This is not a migration mechanism.
use std::collections::{BTreeMap, BTreeSet};
use turso::Connection;

use crate::{StoreError, StoreErrorCode, schema_sql};
mod encrypted;
mod gateway;
mod product;

pub(crate) const ENCRYPTED_LAYOUT_VERSION: i64 = 3;
const MAX_OBJECTS: usize = 512;

#[derive(Clone, Copy)]
pub(crate) struct SchemaObject {
    name: &'static str,
    kind: &'static str,
    ddl: &'static str,
    marker: Option<i64>,
}
impl SchemaObject {
    const fn table(name: &'static str, ddl: &'static str) -> Self {
        Self {
            name,
            kind: "table",
            ddl,
            marker: None,
        }
    }
    const fn marker(name: &'static str, version: i64, ddl: &'static str) -> Self {
        Self {
            name,
            kind: "table",
            ddl,
            marker: Some(version),
        }
    }
    const fn index(name: &'static str, ddl: &'static str) -> Self {
        Self {
            name,
            kind: "index",
            ddl,
            marker: None,
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Layout {
    Product,
    Encrypted,
}
#[derive(Clone, Copy)]
pub(crate) enum Family {
    Product,
    Core,
    Archive,
    Knowledge,
    Access,
    Reviews,
    Actions,
    Bindings,
    Context,
    Conversation,
    ConversationCore,
    Interactions,
    Tasks,
    Cleanup,
    Registry,
    Gateway,
}
impl Family {
    fn objects(self) -> &'static [SchemaObject] {
        match self {
            Self::Product => product::OBJECTS,
            Self::Core => encrypted::CORE,
            Self::Archive => encrypted::ARCHIVE,
            Self::Knowledge => encrypted::KNOWLEDGE,
            Self::Access => encrypted::ACCESS,
            Self::Reviews => encrypted::REVIEWS,
            Self::Actions => encrypted::ACTIONS,
            Self::Bindings => encrypted::BINDINGS,
            Self::Context => encrypted::CONTEXT,
            Self::Conversation => encrypted::CONVERSATION,
            Self::ConversationCore => encrypted::CONVERSATION_CORE,
            Self::Interactions => encrypted::INTERACTIONS,
            Self::Tasks => encrypted::TASKS,
            Self::Cleanup => encrypted::CLEANUP,
            Self::Registry => encrypted::REGISTRY,
            Self::Gateway => gateway::GATEWAY_OBJECTS,
        }
    }
    fn markers(self) -> impl Iterator<Item = (&'static str, i64)> {
        self.objects()
            .iter()
            .filter_map(|object| object.marker.map(|version| (object.name, version)))
    }
}
impl Layout {
    fn families(self) -> &'static [Family] {
        match self {
            Self::Product => &[Family::Product],
            Self::Encrypted => &[
                Family::Core,
                Family::Archive,
                Family::Knowledge,
                Family::Access,
                Family::Reviews,
                Family::Actions,
                Family::Bindings,
                Family::Context,
                Family::Conversation,
                Family::Interactions,
                Family::Tasks,
                Family::Cleanup,
                Family::Registry,
                Family::Gateway,
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SchemaFailure {
    Unsupported {
        object: &'static str,
        reason: &'static str,
    },
    StoredCorrupt,
    Busy,
    Unavailable,
    InvalidDefinition,
}
impl SchemaFailure {
    pub(crate) fn from_database(error: turso::Error) -> Self {
        match error {
            turso::Error::Corrupt(_) | turso::Error::NotAdb(_) => Self::StoredCorrupt,
            turso::Error::Busy(_) | turso::Error::BusySnapshot(_) => Self::Busy,
            _ => Self::Unavailable,
        }
    }
    /// The fresh transaction started empty; a self-mismatch is not old-store evidence.
    pub(crate) fn during_creation(self) -> Self {
        match self {
            Self::Unsupported { .. } => Self::InvalidDefinition,
            other => other,
        }
    }
    pub(crate) fn into_store(self) -> StoreError {
        match self {
            Self::Unsupported { object, reason } => StoreError::new(
                StoreErrorCode::UnsupportedSchema,
                "selected profile database schema is unsupported",
            )
            .with_metadata("schema_object", object)
            .with_metadata("schema_reason", reason),
            Self::StoredCorrupt => StoreError::new(
                StoreErrorCode::StoredDataCorrupt,
                "local database contains corrupt stored data",
            ),
            Self::Busy => StoreError::new(StoreErrorCode::StorageBusy, "local database is busy"),
            Self::Unavailable => {
                StoreError::new(StoreErrorCode::Storage, "local database is unavailable")
            }
            Self::InvalidDefinition => StoreError::new(
                StoreErrorCode::Storage,
                "internal schema definition is invalid",
            ),
        }
    }
    pub(crate) fn into_agent(self) -> floe_kernel::AgentFailure {
        match self {
            Self::Unsupported { .. } => floe_kernel::AgentFailure::UnsupportedVersion,
            Self::StoredCorrupt => floe_kernel::AgentFailure::VaultUnavailable,
            Self::Busy => floe_kernel::AgentFailure::StorageBusy,
            Self::Unavailable | Self::InvalidDefinition => {
                floe_kernel::AgentFailure::StorageUnavailable
            }
        }
    }
}
fn unsupported(object: &'static str, reason: &'static str) -> SchemaFailure {
    SchemaFailure::Unsupported { object, reason }
}

struct StoredObject {
    kind: String,
    table: String,
    ddl: String,
}
async fn inventory(
    connection: &Connection,
) -> Result<BTreeMap<String, StoredObject>, SchemaFailure> {
    let mut rows = connection.query(
        "SELECT type, CASE WHEN length(CAST(name AS BLOB)) <= 128 THEN name ELSE '' END, CASE WHEN length(CAST(tbl_name AS BLOB)) <= 128 THEN tbl_name ELSE '' END, CASE WHEN length(CAST(sql AS BLOB)) <= 65536 THEN sql ELSE '' END FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*' LIMIT 513", (),
    ).await.map_err(SchemaFailure::from_database)?;
    let mut objects = BTreeMap::new();
    while let Some(row) = rows.next().await.map_err(SchemaFailure::from_database)? {
        let name = row.get::<String>(1).map_err(SchemaFailure::from_database)?;
        let stored = StoredObject {
            kind: row.get::<String>(0).map_err(SchemaFailure::from_database)?,
            table: row.get::<String>(2).map_err(SchemaFailure::from_database)?,
            ddl: row.get::<String>(3).map_err(SchemaFailure::from_database)?,
        };
        if name.is_empty() || stored.table.is_empty() || objects.len() == MAX_OBJECTS {
            return Err(SchemaFailure::Unavailable);
        }
        if objects.insert(name, stored).is_some() {
            return Err(unsupported("catalog", "duplicate_object"));
        }
    }
    Ok(objects)
}
fn declarations(layout: Layout) -> Result<(), SchemaFailure> {
    let mut names = BTreeSet::new();
    for family in layout.families() {
        validate_family_declarations(*family, &mut names)?;
    }
    if matches!(layout, Layout::Encrypted) {
        validate_family_declarations(Family::ConversationCore, &mut names)?;
    }
    Ok(())
}

fn validate_family_declarations(
    family: Family,
    names: &mut BTreeSet<&'static str>,
) -> Result<(), SchemaFailure> {
    for object in family.objects() {
        if !names.insert(object.name)
            || !schema_sql::declares_object(object.ddl, object.kind, object.name)
        {
            return Err(SchemaFailure::InvalidDefinition);
        }
    }
    for (table, _) in family.markers() {
        if !family
            .objects()
            .iter()
            .any(|object| object.name == table && object.kind == "table")
        {
            return Err(SchemaFailure::InvalidDefinition);
        }
    }
    Ok(())
}

/// The caller supplies the one fresh-store transaction. No existing layout is repaired.
pub(crate) async fn create(connection: &Connection, layout: Layout) -> Result<(), SchemaFailure> {
    declarations(layout)?;
    if !inventory(connection).await?.is_empty() {
        return Err(unsupported("catalog", "not_empty"));
    }
    for family in layout.families() {
        create_family(connection, *family).await?;
    }
    if matches!(layout, Layout::Encrypted) {
        // This additive family is created in new Vaults. Older encrypted
        // layouts remain openable; the family is initialized on first explicit
        // use of the Conversation Core storage port.
        create_family(connection, Family::ConversationCore).await?;
    }
    Ok(())
}

async fn create_family(connection: &Connection, family: Family) -> Result<(), SchemaFailure> {
    for object in family.objects() {
        connection
            .execute(object.ddl, ())
            .await
            .map_err(SchemaFailure::from_database)?;
    }
    // Core identity includes Person/Vault columns and is seeded by the fresh owner.
    if !matches!(family, Family::Core) {
        for (table, version) in family.markers() {
            connection
                .execute(
                    &format!("INSERT INTO {table}(id,version) VALUES(1,?)"),
                    (version,),
                )
                .await
                .map_err(SchemaFailure::from_database)?;
        }
    }
    Ok(())
}

pub(crate) async fn inspect(connection: &Connection, layout: Layout) -> Result<(), SchemaFailure> {
    declarations(layout)?;
    let stored = inventory(connection).await?;
    let expected = layout
        .families()
        .iter()
        .flat_map(|family| family.objects().iter())
        .map(|object| object.name)
        .collect::<BTreeSet<_>>();
    let mut expected = expected;
    if matches!(layout, Layout::Encrypted) {
        expected.extend(
            Family::ConversationCore
                .objects()
                .iter()
                .map(|object| object.name),
        );
        validate_optional_family_presence(&stored, Family::ConversationCore)?;
    }
    if stored.keys().any(|name| !expected.contains(name.as_str())) {
        return Err(unsupported("catalog", "unexpected_object"));
    }
    for family in layout.families() {
        inspect_declared(connection, *family, &stored).await?;
    }
    if matches!(layout, Layout::Encrypted) && family_present(&stored, Family::ConversationCore) {
        inspect_declared(connection, Family::ConversationCore, &stored).await?;
    }
    Ok(())
}

fn family_present(stored: &BTreeMap<String, StoredObject>, family: Family) -> bool {
    family
        .objects()
        .iter()
        .any(|object| stored.contains_key(object.name))
}

fn validate_optional_family_presence(
    stored: &BTreeMap<String, StoredObject>,
    family: Family,
) -> Result<(), SchemaFailure> {
    let present = family
        .objects()
        .iter()
        .filter(|object| stored.contains_key(object.name))
        .count();
    if present != 0 && present != family.objects().len() {
        return Err(unsupported(
            "agent_conversation_core_schema",
            "partial_family",
        ));
    }
    Ok(())
}

/// Validate the additive family if present without requiring it in an older
/// encrypted Vault. New storage methods call `ensure_conversation_core_family`
/// inside their own immediate transaction before using its tables.
pub(crate) async fn conversation_core_family_present(
    connection: &Connection,
) -> Result<bool, SchemaFailure> {
    let stored = inventory(connection).await?;
    validate_optional_family_presence(&stored, Family::ConversationCore)?;
    if family_present(&stored, Family::ConversationCore) {
        inspect_declared(connection, Family::ConversationCore, &stored).await?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Initialize only the new family in the caller's immediate transaction.
/// Existing Session, journal and receipt rows are never read or rewritten.
pub(crate) async fn ensure_conversation_core_family(
    connection: &Connection,
) -> Result<(), SchemaFailure> {
    if conversation_core_family_present(connection).await? {
        return Ok(());
    }
    create_family(connection, Family::ConversationCore).await?;
    let stored = inventory(connection).await?;
    inspect_declared(connection, Family::ConversationCore, &stored).await
}
pub(crate) async fn inspect_family(
    connection: &Connection,
    family: Family,
) -> Result<(), SchemaFailure> {
    let stored = inventory(connection).await?;
    let layout = if matches!(family, Family::Product) {
        Layout::Product
    } else {
        Layout::Encrypted
    };
    let expected = layout
        .families()
        .iter()
        .flat_map(|family| family.objects().iter())
        .map(|object| object.name)
        .collect::<BTreeSet<_>>();
    let mut expected = expected;
    if matches!(layout, Layout::Encrypted) {
        expected.extend(
            Family::ConversationCore
                .objects()
                .iter()
                .map(|object| object.name),
        );
    }
    if stored.keys().any(|name| !expected.contains(name.as_str())) {
        return Err(unsupported("catalog", "unexpected_object"));
    }
    inspect_declared(connection, family, &stored).await
}
async fn inspect_declared(
    connection: &Connection,
    family: Family,
    stored: &BTreeMap<String, StoredObject>,
) -> Result<(), SchemaFailure> {
    let tables = family
        .objects()
        .iter()
        .filter(|object| object.kind == "table")
        .map(|object| object.name)
        .collect::<BTreeSet<_>>();
    let names = family
        .objects()
        .iter()
        .map(|object| object.name)
        .collect::<BTreeSet<_>>();
    if stored.iter().any(|(name, object)| {
        tables.contains(object.table.as_str()) && !names.contains(name.as_str())
    }) {
        return Err(unsupported("catalog", "unexpected_object"));
    }
    for object in family.objects() {
        let actual = stored
            .get(object.name)
            .ok_or_else(|| unsupported(object.name, "missing_object"))?;
        if actual.kind != object.kind {
            return Err(unsupported(object.name, "object_kind"));
        }
        match schema_sql::compare(&actual.ddl, object.ddl) {
            schema_sql::Comparison::Equivalent => {}
            schema_sql::Comparison::Different => {
                return Err(unsupported(object.name, "definition_mismatch"));
            }
            schema_sql::Comparison::InvalidStored => {
                return Err(unsupported(object.name, "invalid_definition"));
            }
            schema_sql::Comparison::InvalidExpected => {
                return Err(SchemaFailure::InvalidDefinition);
            }
        }
    }
    for (table, expected) in family.markers() {
        let mut rows = connection
            .query(&format!("SELECT id,version FROM {table} LIMIT 2"), ())
            .await
            .map_err(SchemaFailure::from_database)?;
        let row = rows
            .next()
            .await
            .map_err(SchemaFailure::from_database)?
            .ok_or_else(|| unsupported(table, "missing_marker"))?;
        if row.get::<i64>(0).map_err(SchemaFailure::from_database)? != 1
            || row.get::<i64>(1).map_err(SchemaFailure::from_database)? != expected
            || rows
                .next()
                .await
                .map_err(SchemaFailure::from_database)?
                .is_some()
        {
            return Err(unsupported(table, "marker_mismatch"));
        }
    }
    Ok(())
}
