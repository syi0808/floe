use std::time::Duration;

use serde_json::{Value, json};

use crate::{CoreError, ErrorCode, ScheduleModel, ScheduleView};

pub const SCHEDULE_INSTRUCTIONS: &str = "You are Floe's built-in Schedule Expert. Select one supplied unoccupied focus slot. Explain briefly in English using only the supplied busy times and explicit preference. Times are UTC; use timezone_offset_seconds for local times. If preference is null, the 09:00–18:00 / 60-minute window is a default, not a remembered preference. A calendar warning means incomplete or stale data: do not claim full availability. Return only JSON with slot_id (one supplied slot ID), reason (one short sentence), and source_ids (every required_source_id exactly once). Do not claim to create or modify anything. No tools or actions are available.";

pub fn schedule_output_schema(view: &ScheduleView) -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "properties": {
            "slot_id": { "type": "string", "enum": view.slots.iter().map(|slot| &slot.id).collect::<Vec<_>>() },
            "reason": { "type": "string" },
            "source_ids": { "type": "array", "items": { "type": "string", "enum": view.required_source_ids } }
        },
        "required": ["slot_id", "reason", "source_ids"]
    })
}

pub struct GatewayScheduleModel {
    inference_class: String,
    allow_external: bool,
    connection: Option<GatewayConnection>,
}

pub struct GatewayConnection {
    pub base_url: String,
    pub token: String,
}

impl GatewayConnection {
    fn validate(&self) -> Result<(), CoreError> {
        let valid_url = reqwest::Url::parse(&self.base_url).is_ok_and(|url| {
            url.scheme() == "http"
                && url.host_str() == Some("127.0.0.1")
                && url.username().is_empty()
                && url.password().is_none()
                && matches!(url.path(), "" | "/")
                && url.query().is_none()
                && url.fragment().is_none()
                && url.port_or_known_default().is_some_and(|port| port > 0)
        });
        if !valid_url
            || !(32..=256).contains(&self.token.len())
            || !self
                .token
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
        {
            return Err(CoreError::new(
                ErrorCode::Validation,
                "invalid local gateway connection",
            ));
        }
        Ok(())
    }
}

impl GatewayScheduleModel {
    pub fn with_connection(mut self, connection: GatewayConnection) -> Result<Self, CoreError> {
        connection.validate()?;
        self.connection = Some(connection);
        Ok(self)
    }

    pub fn new(inference_class: String, allow_external: bool) -> Result<Self, CoreError> {
        if !matches!(
            inference_class.as_str(),
            "fast" | "balanced" | "high_effort"
        ) {
            return Err(CoreError::new(
                ErrorCode::Validation,
                "invalid inference class",
            ));
        }
        Ok(Self {
            inference_class,
            allow_external,
            connection: None,
        })
    }
}

impl ScheduleModel for GatewayScheduleModel {
    fn name(&self) -> &str {
        &self.inference_class
    }

    async fn generate(&self, view: &ScheduleView) -> Result<String, CoreError> {
        let fallback;
        let connection = if let Some(connection) = &self.connection {
            connection
        } else {
            fallback = GatewayConnection {
                base_url: "http://127.0.0.1:8431".to_owned(),
                token: std::env::var("FLOE_INFERENCE_TOKEN")
                    .ok()
                    .filter(|token| token.len() >= 32)
                    .ok_or_else(|| {
                        CoreError::new(
                            ErrorCode::ModelUnavailable,
                            "configure the local inference gateway",
                        )
                    })?,
            };
            fallback.validate()?;
            &fallback
        };
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(42))
            .connect_timeout(Duration::from_secs(3))
            .build()
            .map_err(transport_error)?;
        let mut response = client
            .post(format!(
                "{}/v1/generate",
                connection.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&connection.token)
            .json(&json!({
                "schema_version": 1,
                "inference_class": self.inference_class,
                "allow_external": self.allow_external,
                "instructions": SCHEDULE_INSTRUCTIONS,
                "input": view,
                "output_schema": schedule_output_schema(view)
            }))
            .send()
            .await
            .map_err(transport_error)?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
            if bytes.len() + chunk.len() > 65_536 {
                return Err(CoreError::new(
                    ErrorCode::InvalidProposal,
                    "gateway response exceeds limit",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let body: Value = serde_json::from_slice(&bytes)
            .map_err(|_| CoreError::new(ErrorCode::InvalidProposal, "invalid gateway response"))?;
        if !status.is_success() {
            let code = match body.pointer("/error/code").and_then(Value::as_str) {
                Some("model_timeout") => ErrorCode::ModelTimeout,
                Some("invalid_proposal") => ErrorCode::InvalidProposal,
                Some("external_transfer_denied") => ErrorCode::ExternalTransferDenied,
                _ => ErrorCode::ModelUnavailable,
            };
            return Err(CoreError::new(
                code,
                "inference gateway request failed; check configuration and retry",
            ));
        }
        if body.get("schema_version") != Some(&json!(1))
            || body.get("inference_class") != Some(&json!(self.inference_class))
        {
            return Err(CoreError::new(
                ErrorCode::InvalidProposal,
                "unexpected gateway response",
            ));
        }
        body.get("output")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                CoreError::new(ErrorCode::InvalidProposal, "gateway response has no output")
            })
    }
}

fn transport_error(error: reqwest::Error) -> CoreError {
    if error.is_timeout() {
        CoreError::new(
            ErrorCode::ModelTimeout,
            "inference gateway request timed out",
        )
    } else {
        CoreError::new(
            ErrorCode::ModelUnavailable,
            "could not reach the local inference gateway",
        )
    }
}
