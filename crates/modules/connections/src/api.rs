#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProducerIdentity {
    pub schema_version: u32,
    pub instance_id: String,
    pub execution_owner: String,
    pub audience: String,
    pub key_id: String,
    pub public_key: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingIssuer {
    pub key_id: String,
    pub public_key: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingConfirmation {
    pub schema_version: u32,
    pub pairing_id: String,
    pub status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingStatus {
    pub schema_version: u32,
    pub pairing_id: String,
    pub status: String,
    pub person_id: String,
    pub device_id: String,
    pub producer: Option<ProducerIdentity>,
    pub issuer: Option<PairingIssuer>,
    pub issuer_fingerprint: Option<String>,
    pub client_id: Option<String>,
    pub token: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingConfirmationRequest {
    pub pairing_id: String,
    pub polling_proof: String,
    pub challenge_id: String,
    pub key_id: String,
    pub signature: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingStatusRequest {
    pub pairing_id: String,
    pub polling_proof: String,
}
