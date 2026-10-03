//! Read a single-use sanitized receipt from the same bundled native host that
//! performed the Health privacy operation. No raw aggregate enters Rust.
use floe_kernel::{AgentFailure,PersonId};
use serde::{Deserialize,Serialize};
use uuid::Uuid;
#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthTransformReceiptRef {pub operation_id:Uuid,pub output_sha256:String}
#[derive(Clone,Debug,Eq,PartialEq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthTransformBinding {pub request_id:Uuid,pub host_epoch:String,pub person_id:PersonId,pub device_id:String,pub native_subject_fingerprint:String}
#[derive(Clone,Debug,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthReceiptOutput {pub capacity:String,pub recovery:String}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptWire {schema_version:u32,status:String,request_id:Option<Uuid>,output:Option<HealthReceiptOutput>,binding:Option<HealthTransformBinding>,output_sha256:Option<String>,transformed_at_unix_ms:Option<i64>,expires_at_unix_ms:Option<i64>,availability:Option<String>,failure:Option<String>}
pub struct HealthPrivacyReceipt {pub output:HealthReceiptOutput,pub transformed_at_unix_ms:i64,pub expires_at_unix_ms:i64}
static HEALTH_TRANSFORM:crate::ByteCall=crate::ByteCall::new(crate::NativeLibrary{
    relative_path:"Frameworks/libfloe_local_model.dylib",invoke_symbol:c"floe_health_privacy_transform",release_symbol:c"floe_health_privacy_transform_free",
    #[cfg(target_os="macos")]bundle_parents:crate::MACOS_BUNDLE_ROOT,
    #[cfg(not(target_os="macos"))]bundle_parents:crate::BUNDLE_SIBLING,
});
pub fn consume_health_transform_receipt(reference:&HealthTransformReceiptRef,binding:&HealthTransformBinding)->Result<HealthPrivacyReceipt,AgentFailure>{
    if reference.operation_id.is_nil()||reference.output_sha256.len()!=64||!reference.output_sha256.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))||binding.request_id.is_nil(){return Err(AgentFailure::PolicyDenied)}
    let request=serde_json::to_vec(&serde_json::json!({"schema_version":1,"operation":"consume_receipt","request_id":reference.operation_id,"output_sha256":reference.output_sha256,"binding":binding})).map_err(|_|AgentFailure::InvalidInput)?;
    let bytes=HEALTH_TRANSFORM.call(&request,8192).map_err(|_|AgentFailure::CapabilityUnavailable)?;
    let response:ReceiptWire=serde_json::from_slice(&bytes).map_err(|_|AgentFailure::PolicyDenied)?;
    if response.schema_version!=1||response.status!="receipt"||response.request_id!=Some(reference.operation_id)||response.binding.as_ref()!=Some(binding)||response.output_sha256.as_ref()!=Some(&reference.output_sha256)||response.availability.is_some()||response.failure.is_some(){return Err(AgentFailure::PolicyDenied)}
    Ok(HealthPrivacyReceipt{output:response.output.ok_or(AgentFailure::PolicyDenied)?,transformed_at_unix_ms:response.transformed_at_unix_ms.ok_or(AgentFailure::PolicyDenied)?,expires_at_unix_ms:response.expires_at_unix_ms.ok_or(AgentFailure::PolicyDenied)?})
}
