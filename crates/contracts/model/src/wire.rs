use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use uuid::Uuid;
use crate::{ModelContractError, ModelOutputFormat, ModelSchema, strict_json, validate_json};

pub const DEVICE_MODEL_VERSION: u32 = 1;
pub const MAX_DEVICE_REQUEST_BYTES: usize = 131_072;
pub const MAX_DEVICE_RESPONSE_BYTES: usize = 65_536;
pub const MAX_DEVICE_INPUT_BYTES: usize = 16_384;
pub const MAX_DEVICE_INSTRUCTIONS_BYTES: usize = 9_216;
pub const MAX_DEVICE_OUTPUT_BYTES: usize = 32_768;
const MAX_SAFE_USAGE: u64 = 9_007_199_254_740_991;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceModelCapability { StructuredOutput, Text, ToolProposals }
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceModelRequirements { pub capabilities: Vec<DeviceModelCapability> }
impl DeviceModelRequirements {
    pub fn validate(&self) -> Result<(), ModelContractError> { validate_capabilities(&self.capabilities) }
}
fn validate_capabilities(values: &[DeviceModelCapability]) -> Result<(), ModelContractError> {
    if values.is_empty() || values.len() > 3 || values.windows(2).any(|pair| pair[0] >= pair[1]) { return Err(ModelContractError::Invalid); }
    Ok(())
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceModelLimits {
    pub max_input_bytes: usize, pub max_instructions_bytes: usize, pub max_output_bytes: usize,
    pub max_response_tokens: u32, pub max_deadline_milliseconds: u32,
}
impl DeviceModelLimits {
    pub fn validate(&self) -> Result<(), ModelContractError> {
        if !(1..=MAX_DEVICE_INPUT_BYTES).contains(&self.max_input_bytes)
            || !(1..=MAX_DEVICE_INSTRUCTIONS_BYTES).contains(&self.max_instructions_bytes)
            || !(1..=MAX_DEVICE_OUTPUT_BYTES).contains(&self.max_output_bytes)
            || !(1..=4096).contains(&self.max_response_tokens)
            || !(1..=30_000).contains(&self.max_deadline_milliseconds)
        { return Err(ModelContractError::Bounds); }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceModelProfile {
    pub binding_id: String, pub capabilities: Vec<DeviceModelCapability>, pub limits: DeviceModelLimits,
}
impl DeviceModelProfile {
    pub fn validate(&self) -> Result<(), ModelContractError> {
        if !valid_binding(&self.binding_id) { return Err(ModelContractError::Invalid); }
        validate_capabilities(&self.capabilities)?;
        self.limits.validate()
    }
    pub fn validate_request(&self, request: &DeviceModelRequest) -> Result<(), ModelContractError> {
        self.validate()?; request.validate()?;
        if request.binding_id != self.binding_id || request.required_capabilities().iter().any(|required| !self.capabilities.contains(required)) { return Err(ModelContractError::Unsupported); }
        if encoded_size(&request.input)? > self.limits.max_input_bytes
            || request.instructions.len() > self.limits.max_instructions_bytes
            || request.max_output_bytes > self.limits.max_output_bytes
            || request.max_response_tokens > self.limits.max_response_tokens
            || request.deadline_milliseconds > self.limits.max_deadline_milliseconds
        { return Err(ModelContractError::Bounds); }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeviceModelObservation {
    Available { binding_id: String, capabilities: Vec<DeviceModelCapability>, limits: DeviceModelLimits },
    Unavailable { reason: DeviceModelUnavailable },
}
impl DeviceModelObservation {
    pub fn profile(self) -> Result<Option<DeviceModelProfile>, ModelContractError> {
        match self {
            Self::Available { binding_id, capabilities, limits } => {
                let profile = DeviceModelProfile { binding_id, capabilities, limits };
                profile.validate()?; Ok(Some(profile))
            }
            Self::Unavailable { .. } => Ok(None),
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceModelUnavailable { Unsupported, Disabled, NotReady }
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceModelFailure { Unsupported, Disabled, NotReady, Unavailable, InvalidInput, InvalidOutput,
    DeadlineExceeded, Cancelled, PolicyDenied, QuotaExceeded, Busy, Conflict, NotFound }

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceTool { pub name: String, pub description: String, pub input_schema: ModelSchema }
impl DeviceTool {
    pub fn validate(&self) -> Result<(), ModelContractError> {
        if self.name.is_empty() || self.name.len() > 64 || !self.name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            || self.description.is_empty() || self.description.len() > 2048
        { return Err(ModelContractError::Invalid); }
        self.input_schema.validate()
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceModelRequest {
    pub operation_id: Uuid, pub binding_id: String, pub instructions: String, pub input: Value,
    pub output_format: ModelOutputFormat, pub tools: Vec<DeviceTool>, pub max_response_tokens: u32,
    pub max_output_bytes: usize, pub deadline_milliseconds: u32,
}
impl DeviceModelRequest {
    pub fn required_capabilities(&self) -> Vec<DeviceModelCapability> {
        let mut capabilities = vec![if self.output_format.is_json() { DeviceModelCapability::StructuredOutput } else { DeviceModelCapability::Text }];
        if !self.tools.is_empty() { capabilities.push(DeviceModelCapability::ToolProposals); }
        capabilities
    }
    pub fn validate(&self) -> Result<(), ModelContractError> {
        if self.operation_id.is_nil() || !valid_binding(&self.binding_id) || self.instructions.trim().is_empty()
            || self.instructions.len() > MAX_DEVICE_INSTRUCTIONS_BYTES || !self.input.is_object()
            || encoded_size(&self.input)? > MAX_DEVICE_INPUT_BYTES || self.tools.len() > 64
            || !(1..=4096).contains(&self.max_response_tokens) || !(1..=MAX_DEVICE_OUTPUT_BYTES).contains(&self.max_output_bytes)
            || !(1..=30_000).contains(&self.deadline_milliseconds) || (self.output_format.is_json() && !self.tools.is_empty())
            || encoded_size(self)? > MAX_DEVICE_REQUEST_BYTES
        { return Err(ModelContractError::Invalid); }
        validate_json(&self.input)?; self.output_format.validate()?;
        let mut names = std::collections::BTreeSet::new();
        for tool in &self.tools { tool.validate()?; if !names.insert(&tool.name) { return Err(ModelContractError::Invalid); } }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceModelUsage {
    #[serde(deserialize_with = "required_nullable")]
    pub tokens: Option<u64>,
    #[serde(deserialize_with = "required_nullable")]
    pub cost_micros: Option<u64>,
}
impl DeviceModelUsage {
    pub fn validate(&self) -> Result<(), ModelContractError> {
        if self.tokens.into_iter().chain(self.cost_micros).any(|n| n > MAX_SAFE_USAGE) { return Err(ModelContractError::Bounds); }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeviceModelOutput {
    Text { text: String }, Json { value: Value }, ToolProposal { name: String, input: Value }, Failure { failure: DeviceModelFailure },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceModelResponse {
    pub operation_id: Uuid, pub binding_id: String, pub output: DeviceModelOutput, pub usage: DeviceModelUsage,
}
impl DeviceModelResponse {
    pub fn validate_envelope(&self, request: &DeviceModelRequest) -> Result<(), ModelContractError> {
        if self.operation_id != request.operation_id || self.binding_id != request.binding_id || encoded_size(self)? > MAX_DEVICE_RESPONSE_BYTES { return Err(ModelContractError::Invalid); }
        self.usage.validate()
    }
    pub fn validate_output(&self, request: &DeviceModelRequest) -> Result<(), ModelContractError> {
        self.validate_envelope(request)?;
        if !matches!(self.output, DeviceModelOutput::Failure { .. }) && encoded_size(&self.output)? > request.max_output_bytes { return Err(ModelContractError::Bounds); }
        match (&self.output, &request.output_format) {
            (DeviceModelOutput::Text { text }, ModelOutputFormat::Text) if !text.trim().is_empty() => Ok(()),
            (DeviceModelOutput::Json { value }, ModelOutputFormat::Json { schema }) => schema.validate_value(value),
            (DeviceModelOutput::ToolProposal { name, input }, ModelOutputFormat::Text) => request.tools.iter().find(|tool| tool.name == *name).ok_or(ModelContractError::Invalid)?.input_schema.validate_value(input),
            (DeviceModelOutput::Failure { .. }, _) => Ok(()),
            _ => Err(ModelContractError::Invalid),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeviceModelCommand {
    Prepare { schema_version: u32, requirements: DeviceModelRequirements },
    Start { schema_version: u32, request: DeviceModelRequest },
    Poll { schema_version: u32, operation_id: Uuid }, Cancel { schema_version: u32, operation_id: Uuid }, Release { schema_version: u32, operation_id: Uuid },
}
impl<'de> Deserialize<'de> for DeviceModelCommand {
    fn deserialize<D:Deserializer<'de>>(deserializer:D) -> Result<Self,D::Error> {
        struct CommandVisitor;
        impl<'de> serde::de::Visitor<'de> for CommandVisitor {
            type Value=DeviceModelCommand;
            fn expecting(&self,f:&mut std::fmt::Formatter<'_>) -> std::fmt::Result {f.write_str("an exact DeviceModel command")}
            fn visit_map<A:serde::de::MapAccess<'de>>(self,mut map:A) -> Result<Self::Value,A::Error> {
                let mut version:Option<u32>=None; let mut operation:Option<String>=None;
                let mut requirements:Option<DeviceModelRequirements>=None;
                let mut request:Option<DeviceModelRequest>=None; let mut id:Option<Uuid>=None;
                while let Some(key)=map.next_key::<String>()? {
                    match key.as_str() {
                        "schema_version" if version.is_none() => version=Some(map.next_value()?),
                        "operation" if operation.is_none() => operation=Some(map.next_value()?),
                        "requirements" if requirements.is_none() => requirements=Some(map.next_value()?),
                        "request" if request.is_none() => request=Some(map.next_value()?),
                        "operation_id" if id.is_none() => id=Some(map.next_value()?),
                        _ => return Err(serde::de::Error::custom("unknown or duplicate DeviceModel command field")),
                    }
                }
                let schema_version=version.filter(|version| *version==DEVICE_MODEL_VERSION)
                    .ok_or_else(|| <A::Error as serde::de::Error>::custom("unsupported DeviceModel version"))?;
                let command=match (operation.as_deref(),requirements,request,id) {
                    (Some("prepare"),Some(requirements),None,None) => {
                        requirements.validate().map_err(serde::de::Error::custom)?;
                        DeviceModelCommand::Prepare {schema_version,requirements}
                    }
                    (Some("start"),None,Some(request),None) => {
                        request.validate().map_err(serde::de::Error::custom)?;
                        DeviceModelCommand::Start {schema_version,request}
                    }
                    (Some("poll"),None,None,Some(operation_id)) if !operation_id.is_nil() => DeviceModelCommand::Poll {schema_version,operation_id},
                    (Some("cancel"),None,None,Some(operation_id)) if !operation_id.is_nil() => DeviceModelCommand::Cancel {schema_version,operation_id},
                    (Some("release"),None,None,Some(operation_id)) if !operation_id.is_nil() => DeviceModelCommand::Release {schema_version,operation_id},
                    _ => return Err(serde::de::Error::custom("invalid DeviceModel command fields")),
                };
                Ok(command)
            }
        }
        deserializer.deserialize_map(CommandVisitor)
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeviceModelReply {
    Observation { schema_version: u32, observation: DeviceModelObservation },
    Pending { schema_version: u32, operation_id: Uuid },
    Done { schema_version: u32, response: DeviceModelResponse },
    Error { schema_version: u32, #[serde(deserialize_with = "required_nullable")] operation_id: Option<Uuid>, failure: DeviceModelFailure },
    Released { schema_version: u32, operation_id: Uuid },
}
impl DeviceModelReply {
    pub fn decode(bytes: &[u8]) -> Result<Self, ModelContractError> {
        if bytes.is_empty() || bytes.len() > MAX_DEVICE_RESPONSE_BYTES { return Err(ModelContractError::Bounds); }
        // A bounded raw content stage keeps acknowledged usage when only model
        // content is malformed. Exact metadata remains independently strict.
        #[derive(Deserialize)]
        struct Status { status: String }
        let status: Status = serde_json::from_slice(bytes).map_err(|_| ModelContractError::Invalid)?;
        if status.status == "done" {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Done { schema_version:u32, status:String, response:Box<serde_json::value::RawValue> }
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct RawResponse {
                operation_id:Uuid, binding_id:String, usage:DeviceModelUsage,
                #[serde(default)]
                output:Option<Box<serde_json::value::RawValue>>,
            }
            let done:Done=serde_json::from_slice(bytes).map_err(|_| ModelContractError::Invalid)?;
            if done.schema_version != DEVICE_MODEL_VERSION || done.status != "done" { return Err(ModelContractError::Unsupported); }
            let raw:RawResponse=serde_json::from_str(done.response.get()).map_err(|_| ModelContractError::Invalid)?;
            if raw.operation_id.is_nil() || !valid_binding(&raw.binding_id) { return Err(ModelContractError::Invalid); }
            raw.usage.validate()?;
            let output=raw.output.and_then(|output| strict_json(output.get().as_bytes(),MAX_DEVICE_RESPONSE_BYTES).ok())
                .and_then(|output| serde_json::from_value(output).ok())
                .unwrap_or(DeviceModelOutput::Failure { failure:DeviceModelFailure::InvalidOutput });
            return Ok(Self::Done { schema_version:done.schema_version,response:DeviceModelResponse {
                operation_id:raw.operation_id,binding_id:raw.binding_id,usage:raw.usage,output,
            } });
        }
        let value = strict_json(bytes, MAX_DEVICE_RESPONSE_BYTES)?;
        let reply: Self = serde_json::from_value(value).map_err(|_| ModelContractError::Invalid)?;
        let version = match &reply {
            Self::Observation { schema_version, .. } | Self::Pending { schema_version, .. } | Self::Done { schema_version, .. }
            | Self::Error { schema_version, .. } | Self::Released { schema_version, .. } => *schema_version,
        };
        if version != DEVICE_MODEL_VERSION { return Err(ModelContractError::Unsupported); }
        Ok(reply)
    }
}
pub fn valid_binding(value: &str) -> bool { value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) }
pub fn encoded_size<T: Serialize>(value: &T) -> Result<usize, ModelContractError> { serde_json::to_vec(value).map(|bytes| bytes.len()).map_err(|_| ModelContractError::Invalid) }
fn required_nullable<'de,D: Deserializer<'de>,T:Deserialize<'de>>(deserializer:D) -> Result<Option<T>,D::Error> { Option::<T>::deserialize(deserializer) }
