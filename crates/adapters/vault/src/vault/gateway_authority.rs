//! Bounded encrypted identity/operation/receipt storage. Pairing lifecycle and
//! source policy belong to Connections and Access, respectively.
use base64::{Engine as _,engine::general_purpose::URL_SAFE_NO_PAD};
use floe_access::{AuthorizationSigner,AuthorizationSigningCommand,AuthorizationSignature,GatewayTrustReader,RemoteProducerIdentity};
use floe_agent_contract::{AgentFailure,BoxFuture};
use floe_connections::*;
use ring::signature;
use sha2::{Digest,Sha256};
use turso::transaction::{Transaction,TransactionBehavior};
use uuid::Uuid;
use super::{EncryptedAgentVault,VaultKeyProvider,storage};
use super::authority_keys::decode_exact;
const OWNER_SIGNATURE_DOMAIN:&[u8]=b"floe.remote.authorization.v1\0";
const PRODUCER_SIGNATURE_DOMAIN:&[u8]=b"floe.remote.producer.v1\0";
const MAX_RECORD:usize=131_072;

impl<K:VaultKeyProvider> EncryptedAgentVault<K>{
    pub(crate) async fn initialize_remote_authority_store(&self,fresh:bool)->Result<(),AgentFailure>{
        if !fresh{self.validate_owner_key().await?;}
        let mut connection=self.connection()?;
        let tx=connection.transaction_with_behavior(TransactionBehavior::Immediate).await.map_err(storage)?;
        let result=async{
            if fresh{
                for sql in [
                    "CREATE TABLE remote_authority_schema(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL)",
                    "CREATE TABLE remote_authority_owner(id INTEGER PRIMARY KEY CHECK(id=1),key_id TEXT NOT NULL,public_key TEXT NOT NULL,nonce TEXT NOT NULL,ciphertext TEXT NOT NULL)",
                    "CREATE TABLE remote_authority_producer(id INTEGER PRIMARY KEY CHECK(id=1),identity_json TEXT NOT NULL,revision INTEGER NOT NULL CHECK(revision>0))",
                    "CREATE TABLE gateway_credential_expectation(id INTEGER PRIMARY KEY CHECK(id=1),payload TEXT NOT NULL)",
                    "CREATE TABLE remote_authority_clock(id INTEGER PRIMARY KEY CHECK(id=1),last_now_unix_ms INTEGER NOT NULL)",
                    "CREATE TABLE gateway_pin_receipts(operation_id TEXT PRIMARY KEY,payload TEXT NOT NULL)",
                    "CREATE TABLE gateway_enrollment_receipts(operation_id TEXT PRIMARY KEY,challenge_id TEXT UNIQUE NOT NULL,command_json TEXT NOT NULL)",
                    "CREATE TABLE gateway_authorization_receipts(operation_id TEXT PRIMARY KEY,challenge_id TEXT UNIQUE NOT NULL,request_digest TEXT NOT NULL,operation TEXT NOT NULL,admission_id TEXT NOT NULL,expectation_json TEXT NOT NULL,expires_at_unix_ms INTEGER NOT NULL)",
                    "CREATE TABLE connections_product_records(record_ref TEXT PRIMARY KEY,person_id TEXT NOT NULL,command_id TEXT NOT NULL,revision INTEGER NOT NULL,payload TEXT NOT NULL,UNIQUE(person_id,command_id))",
                    "CREATE TABLE gateway_setup_receipts(target_ref TEXT PRIMARY KEY,person_id TEXT NOT NULL,command_id TEXT NOT NULL,payload TEXT NOT NULL,UNIQUE(person_id,command_id))",
                    "CREATE TABLE gateway_pairing_operations(operation_id TEXT PRIMARY KEY,person_id TEXT NOT NULL,command_id TEXT NOT NULL,revision INTEGER NOT NULL,state TEXT NOT NULL,payload TEXT NOT NULL,UNIQUE(person_id,command_id))",
                ]{tx.execute(sql,()).await.map_err(storage)?;}
                tx.execute("INSERT INTO remote_authority_schema VALUES(1,1)",()).await.map_err(storage)?;
                tx.execute("INSERT INTO remote_authority_clock VALUES(1,0)",()).await.map_err(storage)?;
                tx.execute("INSERT INTO gateway_credential_expectation VALUES(1,?)",(bounded_encode(&floe_access::GatewayCredentialExpectation::Unpaired)?,)).await.map_err(storage)?;
                let(key,public,nonce,ciphertext)=self.generate_wrapped_owner_key()?;
                tx.execute("INSERT INTO remote_authority_owner VALUES(1,?,?,?,?)",(key,public,nonce,ciphertext)).await.map_err(storage)?;
            }else{
                let mut rows=tx.query("SELECT version FROM remote_authority_schema WHERE id=1",()).await.map_err(storage)?;
                if rows.next().await.map_err(storage)?.ok_or(AgentFailure::VaultUnavailable)?.get::<i64>(0).map_err(storage)?!=1{return Err(AgentFailure::UnsupportedVersion)}
                for table in ["remote_authority_owner","remote_authority_producer","remote_authority_clock","gateway_pin_receipts","gateway_enrollment_receipts","gateway_authorization_receipts","gateway_pairing_operations","gateway_setup_receipts","connections_product_records","gateway_credential_expectation"]{
                    tx.query(&format!("SELECT * FROM {table} LIMIT 0"),()).await.map_err(storage)?;
                }
            }
            Ok(())
        }.await;
        self.finish_access_grant_transaction(tx,result).await
    }
    pub(super) async fn remote_pinned_producer(&self)->Result<RemoteProducerIdentity,AgentFailure>{
        let pin=self.current_pin_record().await?.ok_or(AgentFailure::PolicyDenied)?;Ok(pin.producer)
    }
    async fn current_pin_record(&self)->Result<Option<GatewayPin>,AgentFailure>{
        self.validate_owner_key().await?;
        let mut rows=self.connection()?.query("SELECT identity_json,revision FROM remote_authority_producer WHERE id=1",()).await.map_err(storage)?;
        let Some(row)=rows.next().await.map_err(storage)? else{return Ok(None)};
        let producer:RemoteProducerIdentity=bounded_decode(&row.get::<String>(0).map_err(storage)?)?;
        validate_producer(&producer)?;
        let revision=row.get::<i64>(1).map_err(storage)?;if revision<=0{return Err(AgentFailure::PolicyDenied)}
        Ok(Some(GatewayPin{producer,revision:revision as u64}))
    }
    async fn enrollment_command(&self,id:Uuid)->Result<Option<EnrollmentSigningCommand>,AgentFailure>{
        let mut rows=self.connection()?.query("SELECT command_json FROM gateway_enrollment_receipts WHERE operation_id=?",(id.to_string(),)).await.map_err(storage)?;
        rows.next().await.map_err(storage)?.map(|row|bounded_decode(&row.get::<String>(0).map_err(storage)?)).transpose()
    }
    async fn sign_enrollment_receipt(&self,command:&EnrollmentSigningCommand)->Result<EnrollmentSignature,AgentFailure>{
        self.validate_owner_key().await?;
        let(key,key_id)=self.load_owner_key().await?;
        if key_id!=command.issuer.key_id||self.person_id!=command.person_id{return Err(AgentFailure::PolicyDenied)}
        let mut bytes=Vec::with_capacity(OWNER_SIGNATURE_DOMAIN.len()+command.canonical_bytes.len());bytes.extend_from_slice(OWNER_SIGNATURE_DOMAIN);bytes.extend_from_slice(&command.canonical_bytes);
        let proof=key.sign(&bytes);self.validate_owner_key().await?;
        Ok(EnrollmentSignature{operation_id:command.operation_id,request_digest:command.request_digest,key_id,signature:URL_SAFE_NO_PAD.encode(proof.as_ref())})
    }
    async fn advance_clock(&self,tx:&Transaction<'_>,now:i64)->Result<(),AgentFailure>{
        let mut rows=tx.query("SELECT last_now_unix_ms FROM remote_authority_clock WHERE id=1",()).await.map_err(storage)?;
        let previous=rows.next().await.map_err(storage)?.ok_or(AgentFailure::VaultUnavailable)?.get::<i64>(0).map_err(storage)?;
        if now<previous{return Err(AgentFailure::PolicyDenied)}drop(rows);
        tx.execute("UPDATE remote_authority_clock SET last_now_unix_ms=? WHERE id=1",(now,)).await.map_err(storage)?;Ok(())
    }
}
impl<K:VaultKeyProvider> GatewayTrustReader for EncryptedAgentVault<K>{
    fn credential_expectation<'a>(&'a self)->BoxFuture<'a,Result<floe_access::GatewayCredentialExpectation,AgentFailure>>{Box::pin(async move{
        self.validate_owner_key().await?;
        let mut rows=self.connection()?.query("SELECT payload FROM gateway_credential_expectation WHERE id=1",()).await.map_err(storage)?;
        bounded_decode(&rows.next().await.map_err(storage)?.ok_or(AgentFailure::PolicyDenied)?.get::<String>(0).map_err(storage)?)
    })}
    fn pinned_producer<'a>(&'a self)->BoxFuture<'a,Result<RemoteProducerIdentity,AgentFailure>>{Box::pin(self.remote_pinned_producer())}
}
pub struct VaultEnrollmentSigner<K:VaultKeyProvider>{vault:std::sync::Arc<EncryptedAgentVault<K>>,verifier:std::sync::Arc<dyn EnrollmentProofVerifier>}
impl<K:VaultKeyProvider> VaultEnrollmentSigner<K>{pub fn new(vault:std::sync::Arc<EncryptedAgentVault<K>>,verifier:std::sync::Arc<dyn EnrollmentProofVerifier>)->Self{Self{vault,verifier}}}
impl<K:VaultKeyProvider> EnrollmentSigner for VaultEnrollmentSigner<K>{
    fn public_key<'a>(&'a self)->BoxFuture<'a,Result<floe_access::RemoteOwnerPublicKey,PairingError>>{Box::pin(async move{self.vault.remote_owner_public_key().await.map_err(pairing_storage)})}
    fn sign_enrollment<'a>(&'a self,command:EnrollmentSigningCommand)->BoxFuture<'a,Result<EnrollmentSignature,PairingError>>{Box::pin(async move{
        if let Some(existing)=self.vault.enrollment_command(command.operation_id).await.map_err(pairing_storage)?{
            if existing!=command{return Err(PairingError::Conflict)}return self.vault.sign_enrollment_receipt(&existing).await.map_err(pairing_storage)
        }
        self.verifier.verify(&command)?;
        validate_enrollment(&command,self.vault.person_id)?;
        if command.issuer!=self.vault.remote_owner_public_key().await.map_err(pairing_storage)?{return Err(PairingError::ForeignIdentity)}
        self.vault.validate_owner_key().await.map_err(pairing_storage)?;
        let payload=bounded_encode(&command).map_err(pairing_storage)?;
        let mut connection=self.vault.connection().map_err(pairing_storage)?;let tx=connection.transaction_with_behavior(TransactionBehavior::Immediate).await.map_err(|_|PairingError::StorageUnavailable)?;
        let result=async{
            self.vault.advance_clock(&tx,chrono::Utc::now().timestamp_millis()).await?;
            let mut rows=tx.query("SELECT command_json FROM gateway_enrollment_receipts WHERE operation_id=? OR challenge_id=?",(command.operation_id.to_string(),command.challenge_id.to_string())).await.map_err(storage)?;
            if let Some(row)=rows.next().await.map_err(storage)?{let existing:EnrollmentSigningCommand=bounded_decode(&row.get::<String>(0).map_err(storage)?)?;if existing!=command{return Err(AgentFailure::Conflict)}return Ok(())}drop(rows);
            tx.execute("INSERT INTO gateway_enrollment_receipts VALUES(?,?,?)",(command.operation_id.to_string(),command.challenge_id.to_string(),payload)).await.map_err(storage)?;Ok(())
        }.await;self.vault.finish_access_grant_transaction(tx,result).await.map_err(pairing_storage)?;
        self.vault.sign_enrollment_receipt(&command).await.map_err(pairing_storage)
    })}
    fn readback<'a>(&'a self,id:Uuid)->BoxFuture<'a,Result<Option<EnrollmentSignature>,PairingError>>{Box::pin(async move{
        match self.vault.enrollment_command(id).await.map_err(pairing_storage)?{Some(command)=>Ok(Some(self.vault.sign_enrollment_receipt(&command).await.map_err(pairing_storage)?)),None=>Ok(None)}
    })}
}
impl<K:VaultKeyProvider> GatewayAuthorityRepository for EncryptedAgentVault<K>{
    fn current_pin<'a>(&'a self)->BoxFuture<'a,Result<Option<GatewayPin>,PairingError>>{Box::pin(async move{self.current_pin_record().await.map_err(pairing_storage)})}
    fn readback<'a>(&'a self,id:Uuid)->BoxFuture<'a,Result<Option<GatewayPinReceipt>,PairingError>>{Box::pin(async move{
        self.validate_owner_key().await.map_err(pairing_storage)?;
        let mut rows=self.connection().map_err(pairing_storage)?.query("SELECT payload FROM gateway_pin_receipts WHERE operation_id=?",(id.to_string(),)).await.map_err(|_|PairingError::StorageUnavailable)?;
        rows.next().await.map_err(|_|PairingError::StorageUnavailable)?.map(|row|bounded_decode(&row.get::<String>(0).map_err(storage)?)).transpose().map_err(pairing_storage)
    })}
    fn commit_pin<'a>(&'a self,command:GatewayPinCommit)->BoxFuture<'a,Result<GatewayPinReceipt,PairingError>>{Box::pin(async move{
        if let Some(receipt)=GatewayAuthorityRepository::readback(self,command.operation_id).await?{if receipt.command!=command{return Err(PairingError::Conflict)}return Ok(receipt)}
        let enrolled=self.enrollment_command(command.operation_id).await.map_err(pairing_storage)?.ok_or(PairingError::Conflict)?;
        if enrolled.request_digest!=command.request_digest||enrolled.producer!=command.producer{return Err(PairingError::ChangedProducer)}
        validate_producer(&command.producer).map_err(pairing_storage)?;
        let mut connection=self.connection().map_err(pairing_storage)?;let tx=connection.transaction_with_behavior(TransactionBehavior::Immediate).await.map_err(|_|PairingError::StorageUnavailable)?;
        let result=async{
            self.advance_clock(&tx,chrono::Utc::now().timestamp_millis()).await?;
            let mut rows=tx.query("SELECT payload FROM gateway_credential_expectation WHERE id=1",()).await.map_err(storage)?;
            let expectation:floe_access::GatewayCredentialExpectation=bounded_decode(&rows.next().await.map_err(storage)?.ok_or(AgentFailure::PolicyDenied)?.get::<String>(0).map_err(storage)?)?;drop(rows);
            if expectation!=(floe_access::GatewayCredentialExpectation::Pending{operation_id:command.operation_id}){return Err(AgentFailure::Conflict)}
            let mut rows=tx.query("SELECT revision FROM remote_authority_producer WHERE id=1",()).await.map_err(storage)?;
            let current=rows.next().await.map_err(storage)?.map(|row|row.get::<i64>(0).map_err(storage)).transpose()?.unwrap_or(0);drop(rows);
            if current<0||current as u64!=command.expected_revision{return Err(AgentFailure::Conflict)}
            let revision=command.expected_revision.checked_add(1).filter(|n|*n<=i64::MAX as u64).ok_or(AgentFailure::Conflict)?;
            let producer=bounded_encode(&command.producer)?;
            tx.execute("INSERT INTO remote_authority_producer(id,identity_json,revision) VALUES(1,?,?) ON CONFLICT(id) DO UPDATE SET identity_json=excluded.identity_json,revision=excluded.revision",(producer,revision as i64)).await.map_err(storage)?;
            tx.execute("UPDATE gateway_credential_expectation SET payload=? WHERE id=1",(bounded_encode(&floe_access::GatewayCredentialExpectation::Pending{operation_id:command.operation_id})?,)).await.map_err(storage)?;
            let receipt=GatewayPinReceipt{command,revision};
            tx.execute("INSERT INTO gateway_pin_receipts VALUES(?,?)",(receipt.command.operation_id.to_string(),bounded_encode(&receipt)?)).await.map_err(storage)?;
            Ok(receipt)
        }.await;
        self.finish_access_grant_transaction(tx,result).await.map_err(pairing_storage)
    })}
}

impl<K:VaultKeyProvider> PairingRepository for EncryptedAgentVault<K>{
    fn setup<'a>(&'a self,target:Uuid)->BoxFuture<'a,Result<Option<GatewaySetupRecord>,PairingError>>{Box::pin(async move{
        let mut rows=self.connection().map_err(pairing_storage)?.query("SELECT payload FROM gateway_setup_receipts WHERE target_ref=? AND person_id=?",(target.to_string(),self.person_id.to_string())).await.map_err(|_|PairingError::StorageUnavailable)?;
        rows.next().await.map_err(|_|PairingError::StorageUnavailable)?.map(|row|bounded_decode(&row.get::<String>(0).map_err(storage)?)).transpose().map_err(pairing_storage)
    })}
    fn store_setup<'a>(&'a self,record:GatewaySetupRecord)->BoxFuture<'a,Result<GatewaySetupRecord,PairingError>>{Box::pin(async move{
        if record.person_id!=self.person_id||record.command_id.is_nil()||record.setup.target_ref.is_nil(){return Err(PairingError::ForeignIdentity)}
        let payload=bounded_encode(&record).map_err(pairing_storage)?;
        self.connection().map_err(pairing_storage)?.execute("INSERT OR IGNORE INTO gateway_setup_receipts VALUES(?,?,?,?)",(record.setup.target_ref.to_string(),record.person_id.to_string(),record.command_id.to_string(),payload)).await.map_err(|_|PairingError::StorageUnavailable)?;
        let stored=self.setup(record.setup.target_ref).await?.ok_or(PairingError::StorageUnavailable)?;
        if stored!=record{return Err(PairingError::Conflict)}Ok(stored)
    })}
    fn insert<'a>(&'a self,record:PairingRecord)->BoxFuture<'a,Result<PairingRecord,PairingError>>{Box::pin(async move{
        record.validate()?;if record.person_id!=self.person_id||record.revision!=1||record.state!=PairingState::Pending{return Err(PairingError::ForeignIdentity)}
        let mut connection=self.connection().map_err(pairing_storage)?;let tx=connection.transaction_with_behavior(TransactionBehavior::Immediate).await.map_err(|_|PairingError::StorageUnavailable)?;
        let result=async{
            let mut rows=tx.query("SELECT payload FROM gateway_pairing_operations WHERE operation_id=? OR (person_id=? AND command_id=?)",(record.operation_id.to_string(),record.person_id.to_string(),record.command_id.to_string())).await.map_err(storage)?;
            if let Some(row)=rows.next().await.map_err(storage)?{let current:PairingRecord=bounded_decode(&row.get::<String>(0).map_err(storage)?)?;
                if current.operation_id!=record.operation_id||current.command_id!=record.command_id||current.person_id!=record.person_id||current.device_id!=record.device_id||current.target_ref!=record.target_ref{return Err(AgentFailure::Conflict)}return Ok(current)}drop(rows);
            let mut rows=tx.query("SELECT payload FROM gateway_credential_expectation WHERE id=1",()).await.map_err(storage)?;
            let expectation:floe_access::GatewayCredentialExpectation=bounded_decode(&rows.next().await.map_err(storage)?.ok_or(AgentFailure::PolicyDenied)?.get::<String>(0).map_err(storage)?)?;drop(rows);
            if !matches!(expectation,floe_access::GatewayCredentialExpectation::Unpaired|floe_access::GatewayCredentialExpectation::Forgotten{..}){return Err(AgentFailure::Conflict)}
            tx.execute("UPDATE gateway_credential_expectation SET payload=? WHERE id=1",(bounded_encode(&floe_access::GatewayCredentialExpectation::Pending{operation_id:record.operation_id})?,)).await.map_err(storage)?;
            tx.execute("INSERT INTO gateway_pairing_operations VALUES(?,?,?,?,?,?)",(record.operation_id.to_string(),record.person_id.to_string(),record.command_id.to_string(),record.revision as i64,pairing_state(record.state),bounded_encode(&record)?)).await.map_err(storage)?;Ok(record)
        }.await;self.finish_access_grant_transaction(tx,result).await.map_err(pairing_storage)
    })}
    fn load<'a>(&'a self,id:Uuid)->BoxFuture<'a,Result<Option<PairingRecord>,PairingError>>{Box::pin(async move{
        let mut rows=self.connection().map_err(pairing_storage)?.query("SELECT revision,state,payload FROM gateway_pairing_operations WHERE operation_id=? AND person_id=?",(id.to_string(),self.person_id.to_string())).await.map_err(|_|PairingError::StorageUnavailable)?;
        let Some(row)=rows.next().await.map_err(|_|PairingError::StorageUnavailable)? else{return Ok(None)};
        let record:PairingRecord=bounded_decode(&row.get::<String>(2).map_err(storage).map_err(pairing_storage)?).map_err(pairing_storage)?;record.validate()?;
        if record.operation_id!=id||record.person_id!=self.person_id||record.revision as i64!=row.get::<i64>(0).map_err(storage).map_err(pairing_storage)?||pairing_state(record.state)!=row.get::<String>(1).map_err(storage).map_err(pairing_storage)?{return Err(PairingError::Conflict)}Ok(Some(record))
    })}
    fn compare_and_swap<'a>(&'a self,expected_revision:u64,next:PairingRecord)->BoxFuture<'a,Result<PairingRecord,PairingError>>{Box::pin(async move{
        next.validate()?;
        let mut connection=self.connection().map_err(pairing_storage)?;let tx=connection.transaction_with_behavior(TransactionBehavior::Immediate).await.map_err(|_|PairingError::StorageUnavailable)?;
        let result=async{
            let mut rows=tx.query("SELECT payload FROM gateway_pairing_operations WHERE operation_id=? AND person_id=?",(next.operation_id.to_string(),self.person_id.to_string())).await.map_err(storage)?;
            let current:PairingRecord=bounded_decode(&rows.next().await.map_err(storage)?.ok_or(AgentFailure::Conflict)?.get::<String>(0).map_err(storage)?)?;drop(rows);
            validate_pairing_successor(&current,&next,expected_revision)?;
            if next.state==PairingState::Paired{
                let enrollment=next.enrollment.as_ref().ok_or(AgentFailure::PolicyDenied)?;
                let mut rows=tx.query("SELECT payload FROM gateway_credential_expectation WHERE id=1",()).await.map_err(storage)?;
                let expectation:floe_access::GatewayCredentialExpectation=bounded_decode(&rows.next().await.map_err(storage)?.ok_or(AgentFailure::PolicyDenied)?.get::<String>(0).map_err(storage)?)?;drop(rows);
                if expectation!=(floe_access::GatewayCredentialExpectation::Pending{operation_id:next.operation_id}){return Err(AgentFailure::Conflict)}
                tx.execute("UPDATE gateway_credential_expectation SET payload=? WHERE id=1",(bounded_encode(&floe_access::GatewayCredentialExpectation::Committed{operation_id:next.operation_id,generation:enrollment.binding.credential_generation})?,)).await.map_err(storage)?;
            }

            let changed=tx.execute("UPDATE gateway_pairing_operations SET revision=?,state=?,payload=? WHERE operation_id=? AND revision=?",(next.revision as i64,pairing_state(next.state),bounded_encode(&next)?,next.operation_id.to_string(),expected_revision as i64)).await.map_err(storage)?;
            if changed!=1{return Err(AgentFailure::Conflict)}Ok(next)
        }.await;self.finish_access_grant_transaction(tx,result).await.map_err(pairing_storage)
    })}
    fn pending<'a>(&'a self,person:floe_kernel::PersonId,limit:usize)->BoxFuture<'a,Result<Vec<PairingRecord>,PairingError>>{Box::pin(async move{
        if person!=self.person_id||limit==0||limit>64{return Err(PairingError::InvalidInput)}
        let mut rows=self.connection().map_err(pairing_storage)?.query("SELECT payload FROM gateway_pairing_operations WHERE person_id=? AND state IN ('pending','awaiting_local_confirmation','awaiting_approval','verifying','committing') ORDER BY operation_id LIMIT ?",(person.to_string(),limit as i64)).await.map_err(|_|PairingError::StorageUnavailable)?;
        let mut output=Vec::new();while let Some(row)=rows.next().await.map_err(|_|PairingError::StorageUnavailable)?{let record:PairingRecord=bounded_decode(&row.get::<String>(0).map_err(storage).map_err(pairing_storage)?).map_err(pairing_storage)?;record.validate()?;output.push(record);}Ok(output)
    })}
}
fn validate_pairing_successor(current:&PairingRecord,next:&PairingRecord,expected:u64)->Result<(),AgentFailure>{
    if current.revision!=expected||current.revision.checked_add(1)!=Some(next.revision)||current.state.terminal()
        ||current.operation_id!=next.operation_id||current.command_id!=next.command_id||current.person_id!=next.person_id||current.device_id!=next.device_id
        ||current.target_ref!=next.target_ref||current.generation!=next.generation
        ||current.handle.as_ref().is_some_and(|value|next.handle.as_ref()!=Some(value))
        ||current.reviewed.as_ref().is_some_and(|value|next.reviewed.as_ref()!=Some(value))
        ||current.confirmation_command.is_some_and(|value|next.confirmation_command!=Some(value))
        ||current.cancellation_command.is_some_and(|value|next.cancellation_command!=Some(value))
    {return Err(AgentFailure::Conflict)}
    let allowed=matches!((current.state,next.state),
        (PairingState::Pending,PairingState::AwaitingLocalConfirmation)
        |(PairingState::AwaitingLocalConfirmation,PairingState::AwaitingApproval)
        |(PairingState::AwaitingApproval,PairingState::AwaitingApproval|PairingState::Verifying)
        |(PairingState::Verifying,PairingState::Verifying|PairingState::Committing)
        |(PairingState::Committing,PairingState::Verifying|PairingState::Paired))
        ||next.state==current.state||matches!(next.state,PairingState::Rejected|PairingState::Expired|PairingState::Cancelled|PairingState::RepairRequired);
    if !allowed{return Err(AgentFailure::Conflict)}Ok(())
}
fn pairing_state(state:PairingState)->&'static str{match state{PairingState::Pending=>"pending",PairingState::AwaitingLocalConfirmation=>"awaiting_local_confirmation",PairingState::AwaitingApproval=>"awaiting_approval",PairingState::Verifying=>"verifying",PairingState::Committing=>"committing",PairingState::Paired=>"paired",PairingState::Rejected=>"rejected",PairingState::Expired=>"expired",PairingState::Cancelled=>"cancelled",PairingState::RepairRequired=>"repair_required"}}
fn validate_enrollment(command:&EnrollmentSigningCommand,person:floe_kernel::PersonId)->Result<(),PairingError>{
    let now=chrono::Utc::now().timestamp_millis();
    if command.operation_id.is_nil()||command.challenge_id.is_nil()||command.person_id!=person||command.device_id.is_empty()||command.client_id.is_empty()
        ||command.canonical_bytes.is_empty()||command.canonical_bytes.len()>65536||command.producer_signature.len()!=64
        ||command.request_digest!=<[u8;32]>::from(Sha256::digest(&command.canonical_bytes))
        ||command.issued_at_unix_ms>now.saturating_add(5000)||command.expires_at_unix_ms<=now{return Err(PairingError::Rejected)}
    verify_producer_signature(&command.producer,&command.canonical_bytes,&command.producer_signature).map_err(pairing_storage)
}
fn validate_producer(producer:&RemoteProducerIdentity)->Result<(),AgentFailure>{
    let key=decode_exact(&producer.public_key,32)?;
    if producer.schema_version!=1||producer.audience!=format!("floe.server:{}",producer.instance_id)
        ||[&producer.instance_id,&producer.key_id,&producer.execution_owner].iter().any(|id|Uuid::parse_str(id).is_err()||Uuid::parse_str(id).is_ok_and(|id|id.is_nil()))
        ||producer.fingerprint!=hex(&Sha256::digest(&key)){return Err(AgentFailure::PolicyDenied)}Ok(())
}
fn verify_producer_signature(producer:&RemoteProducerIdentity,bytes:&[u8],proof:&[u8])->Result<(),AgentFailure>{
    validate_producer(producer)?;let key=decode_exact(&producer.public_key,32)?;
    let mut message=Vec::with_capacity(PRODUCER_SIGNATURE_DOMAIN.len()+bytes.len());message.extend_from_slice(PRODUCER_SIGNATURE_DOMAIN);message.extend_from_slice(bytes);
    signature::UnparsedPublicKey::new(&signature::ED25519,key).verify(&message,proof).map_err(|_|AgentFailure::PolicyDenied)
}
fn bounded_decode<T:serde::de::DeserializeOwned>(payload:&str)->Result<T,AgentFailure>{if payload.len()>MAX_RECORD{return Err(AgentFailure::VaultUnavailable)}serde_json::from_str(payload).map_err(|_|AgentFailure::VaultUnavailable)}
fn bounded_encode(value:&impl serde::Serialize)->Result<String,AgentFailure>{let payload=serde_json::to_string(value).map_err(|_|AgentFailure::VaultUnavailable)?;if payload.len()>MAX_RECORD{return Err(AgentFailure::BudgetExceeded)}Ok(payload)}
fn pairing_storage(error:AgentFailure)->PairingError{match error{AgentFailure::Conflict=>PairingError::Conflict,AgentFailure::PolicyDenied=>PairingError::ChangedProducer,_=>PairingError::StorageUnavailable}}
fn hex(bytes:&[u8])->String{bytes.iter().map(|byte|format!("{byte:02x}")).collect()}

pub struct VaultAuthorizationSigner<K:VaultKeyProvider>{vault:std::sync::Arc<EncryptedAgentVault<K>>,verifier:std::sync::Arc<dyn floe_access::AuthorizationProofVerifier>}
impl<K:VaultKeyProvider> VaultAuthorizationSigner<K>{pub fn new(vault:std::sync::Arc<EncryptedAgentVault<K>>,verifier:std::sync::Arc<dyn floe_access::AuthorizationProofVerifier>)->Self{Self{vault,verifier}}}
impl<K:VaultKeyProvider> AuthorizationSigner for VaultAuthorizationSigner<K>{
    fn public_key<'a>(&'a self)->BoxFuture<'a,Result<floe_access::RemoteOwnerPublicKey,AgentFailure>>{Box::pin(self.vault.remote_owner_public_key())}
    fn sign_authorization<'a>(&'a self,command:AuthorizationSigningCommand)->BoxFuture<'a,Result<AuthorizationSignature,AgentFailure>>{Box::pin(async move{
        let owner=self.vault.remote_owner_public_key().await?;
        let claims=self.verifier.verify(&command)?;
        command.validate_claims(&claims,self.vault.person_id,&owner.key_id,chrono::Utc::now().timestamp_millis())?;
        verify_producer_signature(&command.producer,&command.canonical_bytes,&command.producer_signature)?;
        self.vault.validate_owner_key().await?;
        let grant_id=floe_access::GrantId::from_uuid(Uuid::parse_str(&command.expected.grant_id).map_err(|_|AgentFailure::PolicyDenied)?).ok_or(AgentFailure::PolicyDenied)?;
        let mut connection=self.vault.connection()?;let tx=connection.transaction_with_behavior(TransactionBehavior::Immediate).await.map_err(storage)?;
        let result=async{
            let now=chrono::Utc::now().timestamp_millis();self.vault.advance_clock(&tx,now).await?;
            if command.expires_at_unix_ms<=now{return Err(AgentFailure::PolicyDenied)}
            let mut pins=tx.query("SELECT identity_json FROM remote_authority_producer WHERE id=1",()).await.map_err(storage)?;
            let pinned:RemoteProducerIdentity=bounded_decode(&pins.next().await.map_err(storage)?.ok_or(AgentFailure::PolicyDenied)?.get::<String>(0).map_err(storage)?)?;
            if pinned!=command.producer{return Err(AgentFailure::PolicyDenied)}drop(pins);
            let grant=self.vault.read_data_access_grant_in_transaction(&tx,grant_id).await?;
            floe_access::validate_authorization_grant(&grant,self.vault.person_id,&command.expected,command.purpose,&command.consumer)?;
            if command.expected.operation=="release"{
                let mut rows=tx.query("SELECT expectation_json,expires_at_unix_ms FROM gateway_authorization_receipts WHERE challenge_id=? AND operation='admission'",(command.expected.admission_id.clone(),)).await.map_err(storage)?;
                let row=rows.next().await.map_err(storage)?.ok_or(AgentFailure::PolicyDenied)?;
                let admitted:floe_access::RemoteViewAuthorizationExpectation=bounded_decode(&row.get::<String>(0).map_err(storage)?)?;
                if row.get::<i64>(1).map_err(storage)?<=now||!same_release(&admitted,&command.expected){return Err(AgentFailure::PolicyDenied)}
            }
            // Single use applies to authorization challenges. This is deliberately
            // distinct from enrollment receipt readback, which can rejoin a proof.
            let mut prior=tx.query("SELECT operation_id FROM gateway_authorization_receipts WHERE operation_id=? OR challenge_id=?",(command.operation_id.to_string(),command.expected.challenge_id.clone())).await.map_err(storage)?;
            if prior.next().await.map_err(storage)?.is_some(){return Err(AgentFailure::Conflict)}drop(prior);
            tx.execute("INSERT INTO gateway_authorization_receipts VALUES(?,?,?,?,?,?,?)",(command.operation_id.to_string(),command.expected.challenge_id.clone(),hex(&command.request_digest),command.expected.operation.clone(),command.expected.admission_id.clone(),bounded_encode(&command.expected)?,command.expires_at_unix_ms)).await.map_err(storage)?;
            Ok(())
        }.await;self.vault.finish_access_grant_transaction(tx,result).await?;
        self.vault.validate_owner_key().await?;
        let(key,key_id)=self.vault.load_owner_key().await?;
        if key_id!=owner.key_id{return Err(AgentFailure::PolicyDenied)}
        let mut bytes=Vec::with_capacity(OWNER_SIGNATURE_DOMAIN.len()+command.canonical_bytes.len());bytes.extend_from_slice(OWNER_SIGNATURE_DOMAIN);bytes.extend_from_slice(&command.canonical_bytes);
        let signature=key.sign(&bytes);self.vault.validate_owner_key().await?;
        Ok(AuthorizationSignature{key_id,signature:URL_SAFE_NO_PAD.encode(signature.as_ref())})
    })}
}
fn same_release(admitted:&floe_access::RemoteViewAuthorizationExpectation,released:&floe_access::RemoteViewAuthorizationExpectation)->bool{
    admitted.operation=="admission"&&released.operation=="release"&&admitted.challenge_id==released.admission_id&&admitted.result_sha256.is_empty()
        &&admitted.client_id==released.client_id&&admitted.device_id==released.device_id&&admitted.query_sha256==released.query_sha256
        &&admitted.grant_id==released.grant_id&&admitted.grant_incarnation==released.grant_incarnation&&admitted.grant_epoch==released.grant_epoch
        &&admitted.source_connector==released.source_connector&&admitted.source_connection==released.source_connection&&admitted.source_execution_owner==released.source_execution_owner
        &&admitted.source_incarnation==released.source_incarnation&&admitted.source_epoch==released.source_epoch&&admitted.resources==released.resources
        &&admitted.max_items==released.max_items&&admitted.max_bytes==released.max_bytes
}

impl<K:VaultKeyProvider> ConnectionsProductRepository for EncryptedAgentVault<K>{
    fn load<'a>(&'a self,person:floe_kernel::PersonId,id:Uuid)->BoxFuture<'a,Result<Option<ConnectionsRecord>,AgentFailure>>{Box::pin(async move{
        if person!=self.person_id{return Err(AgentFailure::PolicyDenied)}
        let mut rows=self.connection()?.query("SELECT revision,payload FROM connections_product_records WHERE record_ref=? AND person_id=?",(id.to_string(),person.to_string())).await.map_err(storage)?;
        let Some(row)=rows.next().await.map_err(storage)? else{return Ok(None)};
        let record:ConnectionsRecord=bounded_decode(&row.get::<String>(1).map_err(storage)?)?;record.validate()?;
        if record.record_ref!=id||record.person_id!=person||record.revision as i64!=row.get::<i64>(0).map_err(storage)?{return Err(AgentFailure::PolicyDenied)}Ok(Some(record))
    })}
    fn list<'a>(&'a self,person:floe_kernel::PersonId,limit:usize)->BoxFuture<'a,Result<Vec<ConnectionsRecord>,AgentFailure>>{Box::pin(async move{
        if person!=self.person_id||limit==0||limit>1024{return Err(AgentFailure::InvalidInput)}
        let mut rows=self.connection()?.query("SELECT record_ref,revision,payload FROM connections_product_records WHERE person_id=? ORDER BY record_ref LIMIT ?",(person.to_string(),limit as i64)).await.map_err(storage)?;
        let mut records=Vec::new();while let Some(row)=rows.next().await.map_err(storage)?{let record:ConnectionsRecord=bounded_decode(&row.get::<String>(2).map_err(storage)?)?;record.validate()?;
            if record.record_ref.to_string()!=row.get::<String>(0).map_err(storage)?||record.revision as i64!=row.get::<i64>(1).map_err(storage)?||record.person_id!=person{return Err(AgentFailure::PolicyDenied)}records.push(record)}Ok(records)
    })}
    fn insert<'a>(&'a self,record:ConnectionsRecord)->BoxFuture<'a,Result<ConnectionsRecord,AgentFailure>>{Box::pin(async move{
        record.validate()?;if record.person_id!=self.person_id||record.revision!=1{return Err(AgentFailure::PolicyDenied)}
        let mut connection=self.connection()?;let tx=connection.transaction_with_behavior(TransactionBehavior::Immediate).await.map_err(storage)?;
        let result=async{
            let mut rows=tx.query("SELECT payload FROM connections_product_records WHERE record_ref=? OR (person_id=? AND command_id=?)",(record.record_ref.to_string(),record.person_id.to_string(),record.command_id.to_string())).await.map_err(storage)?;
            if let Some(row)=rows.next().await.map_err(storage)?{let current:ConnectionsRecord=bounded_decode(&row.get::<String>(0).map_err(storage)?)?;
                if current.record_ref!=record.record_ref||current.command_id!=record.command_id||current.intent_digest!=record.intent_digest||current.person_id!=record.person_id||current.device_id!=record.device_id{return Err(AgentFailure::Conflict)}return Ok(current)}drop(rows);
            if let ConnectionsPayload::GatewayForgotten(summary)=&record.payload{
                let mut rows=tx.query("SELECT payload FROM gateway_credential_expectation WHERE id=1",()).await.map_err(storage)?;
                let current:floe_access::GatewayCredentialExpectation=bounded_decode(&rows.next().await.map_err(storage)?.ok_or(AgentFailure::PolicyDenied)?.get::<String>(0).map_err(storage)?)?;drop(rows);
                let (operation_id,generation)=match current{
                    floe_access::GatewayCredentialExpectation::Committed{operation_id,generation}|floe_access::GatewayCredentialExpectation::Forgotten{operation_id,generation}=>(operation_id,generation),
                    floe_access::GatewayCredentialExpectation::Pending{operation_id}=>(operation_id,1),
                    floe_access::GatewayCredentialExpectation::Unpaired=>(summary.gateway_ref,1),
                };
                if operation_id!=summary.gateway_ref||generation.checked_add(1)!=Some(summary.revision){return Err(AgentFailure::Conflict)}
                tx.execute("UPDATE gateway_credential_expectation SET payload=? WHERE id=1",(bounded_encode(&floe_access::GatewayCredentialExpectation::Forgotten{operation_id,generation})?,)).await.map_err(storage)?;
            }
            tx.execute("INSERT INTO connections_product_records VALUES(?,?,?,?,?)",(record.record_ref.to_string(),record.person_id.to_string(),record.command_id.to_string(),record.revision as i64,bounded_encode(&record)?)).await.map_err(storage)?;Ok(record)
        }.await;self.finish_access_grant_transaction(tx,result).await
    })}
    fn compare_and_swap<'a>(&'a self,expected_revision:u64,record:ConnectionsRecord)->BoxFuture<'a,Result<ConnectionsRecord,AgentFailure>>{Box::pin(async move{
        record.validate()?;if record.person_id!=self.person_id{return Err(AgentFailure::PolicyDenied)}
        let mut connection=self.connection()?;let tx=connection.transaction_with_behavior(TransactionBehavior::Immediate).await.map_err(storage)?;
        let result=async{
            let mut rows=tx.query("SELECT payload FROM connections_product_records WHERE record_ref=? AND person_id=?",(record.record_ref.to_string(),record.person_id.to_string())).await.map_err(storage)?;
            let current:ConnectionsRecord=bounded_decode(&rows.next().await.map_err(storage)?.ok_or(AgentFailure::Conflict)?.get::<String>(0).map_err(storage)?)?;drop(rows);
            if current.revision!=expected_revision{return Err(AgentFailure::Conflict)}current.validate_successor(&record)?;
            let changed=tx.execute("UPDATE connections_product_records SET revision=?,payload=? WHERE record_ref=? AND revision=?",(record.revision as i64,bounded_encode(&record)?,record.record_ref.to_string(),expected_revision as i64)).await.map_err(storage)?;
            if changed!=1{return Err(AgentFailure::Conflict)}Ok(record)
        }.await;self.finish_access_grant_transaction(tx,result).await
    })}
}
