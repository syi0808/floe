use floe_kernel::{AgentFailure, PersonId};

pub struct ReadAuthorityIdentity<'a, Provider> {
    pub person_id: PersonId,
    pub device_id: &'a str,
    pub provider: &'a Provider,
    pub resource_ids: &'a [String],
}

pub struct ReadAuthorityEvidence<'a, Provider> {
    pub schema_version: u32,
    pub identity: ReadAuthorityIdentity<'a, Provider>,
    pub subject_fingerprint: &'a str,
    pub generation: &'a str,
}

pub fn validate_read_authority<Provider: PartialEq>(
    expected: &ReadAuthorityIdentity<'_, Provider>,
    actual: &ReadAuthorityEvidence<'_, Provider>,
) -> Result<(), AgentFailure> {
    if actual.schema_version != 1
        || actual.identity.person_id != expected.person_id
        || actual.identity.device_id != expected.device_id
        || actual.identity.provider != expected.provider
        || actual.identity.resource_ids != expected.resource_ids
        || actual.subject_fingerprint.trim().is_empty()
        || actual.subject_fingerprint.len() != 64
        || actual
            .subject_fingerprint
            .bytes()
            .any(|byte| !byte.is_ascii_hexdigit())
        || actual.generation.is_empty()
        || actual.generation.len() > 128
    {
        return Err(AgentFailure::CapabilityDenied);
    }
    Ok(())
}

pub fn validate_read_continuity(
    previous_fingerprint: &str,
    previous_generation: &str,
    current_fingerprint: &str,
    current_generation: &str,
    allow_generation_change: bool,
) -> Result<(), AgentFailure> {
    if previous_fingerprint != current_fingerprint
        || (!allow_generation_change && previous_generation != current_generation)
    {
        return Err(AgentFailure::StaleContext);
    }
    Ok(())
}
