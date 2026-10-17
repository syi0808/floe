//! A reference to an immutable Person-facing history prefix.
//! This value neither authorizes a reader nor proves membership in storage.
//! Conversation alone owns the append chain and validates stored membership.
use floe_kernel::AgentFailure;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceAnchor {
    pub session_id: Uuid,
    pub sequence: u64,
    pub digest: [u8; 32],
}
impl SurfaceAnchor {
    pub fn empty(session_id: Uuid) -> Result<Self, AgentFailure> {
        if session_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut hash = Sha256::new();
        hash.update(b"floe.surface.prefix\0");
        hash.update(session_id.as_bytes());
        Ok(Self {
            session_id,
            sequence: 0,
            digest: hash.finalize().into(),
        })
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.session_id.is_nil() || self.sequence > i64::MAX as u64 {
            return Err(AgentFailure::InvalidInput);
        }
        if self.sequence == 0 && *self != Self::empty(self.session_id)? {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_prefix_keeps_published_digest_and_wire_shape() {
        let id = Uuid::parse_str("00112233-4455-6677-8899-aabbccddeeff").unwrap();
        let anchor = SurfaceAnchor::empty(id).unwrap();
        assert_eq!(
            anchor.digest,
            [
                170, 21, 174, 203, 11, 91, 143, 82, 232, 248, 93, 38, 63, 38, 240, 56, 33, 128, 35,
                89, 132, 174, 7, 126, 58, 30, 25, 240, 189, 65, 40, 46
            ]
        );
        let json = serde_json::to_value(anchor).unwrap();
        assert_eq!(json.as_object().unwrap().len(), 3);
        assert_eq!(json["session_id"], id.to_string());
        assert_eq!(json["sequence"], 0);
        assert_eq!(
            serde_json::from_value::<SurfaceAnchor>(json).unwrap(),
            anchor
        );
        anchor.validate().unwrap();
    }

    #[test]
    fn rejects_invalid_identity_bounds_and_forged_empty_prefix() {
        assert_eq!(
            SurfaceAnchor::empty(Uuid::nil()),
            Err(AgentFailure::InvalidInput)
        );
        let mut anchor = SurfaceAnchor::empty(Uuid::new_v4()).unwrap();
        anchor.digest[0] ^= 1;
        assert_eq!(anchor.validate(), Err(AgentFailure::Conflict));
        anchor.sequence = i64::MAX as u64 + 1;
        assert_eq!(anchor.validate(), Err(AgentFailure::InvalidInput));
        anchor.sequence = 1;
        anchor.session_id = Uuid::nil();
        assert_eq!(anchor.validate(), Err(AgentFailure::InvalidInput));
    }

    #[test]
    fn validation_of_nonempty_reference_does_not_claim_storage_membership() {
        SurfaceAnchor {
            session_id: Uuid::new_v4(),
            sequence: i64::MAX as u64,
            digest: [0; 32],
        }
        .validate()
        .unwrap();
        let mut json = serde_json::to_value(SurfaceAnchor::empty(Uuid::new_v4()).unwrap()).unwrap();
        json["authorized"] = serde_json::json!(true);
        assert!(serde_json::from_value::<SurfaceAnchor>(json).is_err());
    }
}
