use std::{collections::BTreeSet, fmt};
use serde::{Deserialize, Deserializer, Serialize, de::{MapAccess, SeqAccess, Visitor}};
use serde_json::{Map, Number, Value};

pub const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
const MAX_SCHEMA_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelContractError { Invalid, Bounds, Unsupported }
impl fmt::Display for ModelContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{self:?}") }
}
impl std::error::Error for ModelContractError {}

/// Closed portable schema. Construction and deserialization validate the whole tree.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ModelSchema(Value);
impl<'de> Deserialize<'de> for ModelSchema {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<serde_json::value::RawValue>::deserialize(deserializer)?;
        Self::parse(raw.get().as_bytes()).map_err(serde::de::Error::custom)
    }
}
impl ModelSchema {
    pub fn new(value: Value) -> Result<Self, ModelContractError> {
        validate_json(&value)?;
        if serde_json::to_vec(&value).map_err(|_| ModelContractError::Invalid)?.len() > MAX_SCHEMA_BYTES {
            return Err(ModelContractError::Bounds);
        }
        let mut nodes = 0;
        validate_schema(&value, 0, &mut nodes)?;
        let kind = node_type(&value)?;
        if kind != Kind::Object { return Err(ModelContractError::Invalid); }
        Ok(Self(value))
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, ModelContractError> { Self::new(strict_json(bytes, MAX_SCHEMA_BYTES)?) }
    pub fn as_value(&self) -> &Value { &self.0 }
    pub fn validate(&self) -> Result<(), ModelContractError> { Self::new(self.0.clone()).map(|_| ()) }
    pub fn validate_value(&self, value: &Value) -> Result<(), ModelContractError> {
        validate_json(value)?;
        validate_value(&self.0, value)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelOutputFormat { Text, Json { schema: ModelSchema } }
impl ModelOutputFormat {
    pub fn validate(&self) -> Result<(), ModelContractError> {
        match self { Self::Text => Ok(()), Self::Json { schema } => schema.validate() }
    }
    pub fn is_json(&self) -> bool { matches!(self, Self::Json { .. }) }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind { Object, Array, String, Integer, Number, Boolean }
fn named_type(value: &str) -> Result<Kind, ModelContractError> {
    match value {
        "object" => Ok(Kind::Object), "array" => Ok(Kind::Array), "string" => Ok(Kind::String),
        "integer" => Ok(Kind::Integer), "number" => Ok(Kind::Number), "boolean" => Ok(Kind::Boolean),
        _ => Err(ModelContractError::Unsupported),
    }
}
fn node_type(node: &Value) -> Result<Kind, ModelContractError> {
    let object = node.as_object().ok_or(ModelContractError::Invalid)?;
    match object.get("type") {
        Some(Value::String(name)) => named_type(name),
        Some(Value::Array(_)) => Err(ModelContractError::Unsupported),
        None => match object.get("const") {
            Some(Value::String(_)) => Ok(Kind::String),
            Some(Value::Number(number)) => Ok(if number.as_f64().is_some_and(|n| n.fract() == 0.0) { Kind::Integer } else { Kind::Number }),
            Some(Value::Bool(_)) => Ok(Kind::Boolean),
            _ => Err(ModelContractError::Invalid),
        },
        _ => Err(ModelContractError::Invalid),
    }
}
fn safe_number(value: &Value) -> Result<f64, ModelContractError> {
    let number = value.as_f64().ok_or(ModelContractError::Invalid)?;
    if !number.is_finite() || number.abs() > MAX_SAFE_INTEGER { return Err(ModelContractError::Bounds); }
    Ok(number)
}
fn size_bound(object: &Map<String, Value>, name: &str) -> Result<Option<usize>, ModelContractError> {
    object.get(name).map(|value| {
        let number = safe_number(value)?;
        if number < 0.0 || number > 32768.0 || number.fract() != 0.0 { return Err(ModelContractError::Invalid); }
        Ok(number as usize)
    }).transpose()
}
fn bounds(object: &Map<String, Value>, lower: &str, upper: &str) -> Result<(), ModelContractError> {
    let low = size_bound(object, lower)?;
    let high = size_bound(object, upper)?;
    if low.zip(high).is_some_and(|(a,b)| a > b) { return Err(ModelContractError::Invalid); }
    Ok(())
}
fn identifier(value: &str) -> bool { !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control) }
fn validate_schema(node: &Value, depth: usize, nodes: &mut usize) -> Result<(), ModelContractError> {
    *nodes += 1;
    if depth > 8 || *nodes > 256 { return Err(ModelContractError::Bounds); }
    let object = node.as_object().ok_or(ModelContractError::Invalid)?;
    let kind = node_type(node)?;
    let specific: &[&str] = if !object.contains_key("type") { &[] } else { match kind {
        Kind::Object => &["properties", "required", "additionalProperties"],
        Kind::Array => &["items", "minItems", "maxItems"],
        Kind::String => &["enum", "minLength", "maxLength"],
        Kind::Integer | Kind::Number => &["minimum", "maximum"],
        Kind::Boolean => &[],
    } };
    if object.keys().any(|key| !["type", "description", "const"].contains(&key.as_str()) && !specific.contains(&key.as_str())) {
        return Err(ModelContractError::Unsupported);
    }
    if let Some(description) = object.get("description") {
        if description.as_str().is_none_or(|text| text.len() > 2048) { return Err(ModelContractError::Invalid); }
    }
    match kind {
        Kind::Object => {
            if object.get("additionalProperties") != Some(&Value::Bool(false)) { return Err(ModelContractError::Invalid); }
            let properties = object.get("properties").and_then(Value::as_object).ok_or(ModelContractError::Invalid)?;
            if properties.len() > 64 || properties.keys().any(|key| !identifier(key)) { return Err(ModelContractError::Bounds); }
            let required = object.get("required").and_then(Value::as_array).ok_or(ModelContractError::Invalid)?;
            let mut names = BTreeSet::new();
            for name in required {
                let name = name.as_str().ok_or(ModelContractError::Invalid)?;
                if !properties.contains_key(name) || !names.insert(name) { return Err(ModelContractError::Invalid); }
            }
            for property in properties.values() { validate_schema(property, depth + 1, nodes)?; }
        }
        Kind::Array => {
            bounds(object, "minItems", "maxItems")?;
            validate_schema(object.get("items").ok_or(ModelContractError::Invalid)?, depth + 1, nodes)?;
        }
        Kind::String => {
            bounds(object, "minLength", "maxLength")?;
            if let Some(choices) = object.get("enum") {
                let choices = choices.as_array().ok_or(ModelContractError::Invalid)?;
                if choices.is_empty() || choices.len() > 64 { return Err(ModelContractError::Bounds); }
                let mut values = BTreeSet::new();
                for value in choices {
                    let text = value.as_str().ok_or(ModelContractError::Invalid)?;
                    if !values.insert(text) { return Err(ModelContractError::Invalid); }
                }
            }
        }
        Kind::Integer | Kind::Number => {
            let low = object.get("minimum").map(safe_number).transpose()?;
            let high = object.get("maximum").map(safe_number).transpose()?;
            if low.zip(high).is_some_and(|(a,b)| a > b) || (kind == Kind::Integer && low.into_iter().chain(high).any(|n| n.fract() != 0.0)) { return Err(ModelContractError::Invalid); }
        }
        Kind::Boolean => {}
    }
    if let Some(constant) = object.get("const") {
        if constant.is_null() || constant.is_object() || constant.is_array() { return Err(ModelContractError::Unsupported); }
        validate_value(node, constant)?;
    }
    Ok(())
}
fn matches_size(count: usize, object: &Map<String, Value>, lower: &str, upper: &str) -> Result<bool, ModelContractError> {
    Ok(size_bound(object, lower)?.is_none_or(|n| count >= n) && size_bound(object, upper)?.is_none_or(|n| count <= n))
}
fn equal_scalar(left: &Value, right: &Value) -> bool {
    if left.is_number() && right.is_number() { left.as_f64() == right.as_f64() } else { left == right }
}
fn validate_value(schema: &Value, value: &Value) -> Result<(), ModelContractError> {
    let object = schema.as_object().ok_or(ModelContractError::Invalid)?;
    let kind = node_type(schema)?;
    if let Some(constant) = object.get("const") { if !equal_scalar(constant, value) { return Err(ModelContractError::Invalid); } }
    if value.is_null() { return Err(ModelContractError::Invalid); }
    let valid = match kind {
        Kind::Object => {
            let values = value.as_object().ok_or(ModelContractError::Invalid)?;
            let properties = object.get("properties").and_then(Value::as_object).ok_or(ModelContractError::Invalid)?;
            let required = object.get("required").and_then(Value::as_array).ok_or(ModelContractError::Invalid)?;
            if required.iter().any(|key| key.as_str().is_none_or(|key| !values.contains_key(key))) { return Err(ModelContractError::Invalid); }
            for (key, value) in values { validate_value(properties.get(key).ok_or(ModelContractError::Invalid)?, value)?; }
            true
        }
        Kind::Array => {
            let values = value.as_array().ok_or(ModelContractError::Invalid)?;
            if !matches_size(values.len(), object, "minItems", "maxItems")? { return Err(ModelContractError::Invalid); }
            for value in values { validate_value(object.get("items").ok_or(ModelContractError::Invalid)?, value)?; }
            true
        }
        Kind::String => {
            let text = value.as_str().ok_or(ModelContractError::Invalid)?;
            matches_size(text.chars().count(), object, "minLength", "maxLength")?
                && object.get("enum").is_none_or(|choices| choices.as_array().is_some_and(|choices| choices.contains(value)))
        }
        Kind::Integer | Kind::Number => {
            let number = safe_number(value)?;
            (kind != Kind::Integer || number.fract() == 0.0)
                && object.get("minimum").is_none_or(|bound| bound.as_f64().is_some_and(|bound| number >= bound))
                && object.get("maximum").is_none_or(|bound| bound.as_f64().is_some_and(|bound| number <= bound))
        }
        Kind::Boolean => value.is_boolean(),
    };
    if valid { Ok(()) } else { Err(ModelContractError::Invalid) }
}

pub fn validate_json(value: &Value) -> Result<(), ModelContractError> {
    fn visit(value: &Value, depth: usize) -> Result<(), ModelContractError> {
        if depth > 32 { return Err(ModelContractError::Bounds); }
        match value {
            Value::Number(number) => { safe_number(value)?; validate_number_tokens(number.to_string().as_bytes())?; }
            Value::Array(values) => for value in values { visit(value, depth + 1)?; },
            Value::Object(values) => for value in values.values() { visit(value, depth + 1)?; },
            _ => {}
        }
        Ok(())
    }
    visit(value, 0)
}
pub fn strict_json(bytes: &[u8], maximum: usize) -> Result<Value, ModelContractError> {
    if bytes.is_empty() || bytes.len() > maximum { return Err(ModelContractError::Bounds); }
    validate_number_tokens(bytes)?;
    let value = serde_json::from_slice::<UniqueValue>(bytes).map_err(|_| ModelContractError::Invalid)?.0;
    validate_json(&value)?;
    Ok(value)
}
struct UniqueValue(Value);
impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = UniqueValue;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result { formatter.write_str("bounded JSON with unique keys") }
            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value,E> { Ok(UniqueValue(Value::Bool(value))) }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value,E> { if value.unsigned_abs() > MAX_SAFE_INTEGER as u64 { return Err(E::custom("number outside safe range")); } Ok(UniqueValue(Value::Number(value.into()))) }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value,E> { if value > MAX_SAFE_INTEGER as u64 { return Err(E::custom("number outside safe range")); } Ok(UniqueValue(Value::Number(value.into()))) }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value,E> { if !value.is_finite() || value.abs() > MAX_SAFE_INTEGER { return Err(E::custom("number outside safe range")); } Ok(UniqueValue(Value::Number(Number::from_f64(value).ok_or_else(|| E::custom("nonfinite number"))?))) }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value,E> { Ok(UniqueValue(Value::String(value.to_owned()))) }
            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value,E> { Ok(UniqueValue(Value::String(value))) }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value,E> { Ok(UniqueValue(Value::Null)) }
            fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value,E> { Ok(UniqueValue(Value::Null)) }
            fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value,A::Error> { let mut values=Vec::new(); while let Some(value)=sequence.next_element::<UniqueValue>()? { values.push(value.0); } Ok(UniqueValue(Value::Array(values))) }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value,A::Error> { let mut values=Map::new(); while let Some(key)=map.next_key::<String>()? { if values.contains_key(&key) { return Err(serde::de::Error::custom("duplicate JSON key")); } values.insert(key,map.next_value::<UniqueValue>()?.0); } Ok(UniqueValue(Value::Object(values))) }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}

// Numeric tokens are checked before serde/f64 can erase significant decimals.
fn validate_number_tokens(bytes: &[u8]) -> Result<(), ModelContractError> {
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] == b'"' {
            cursor += 1;
            while cursor < bytes.len() {
                match bytes[cursor] { b'\\' => { cursor += 2; }, b'"' => { cursor += 1; break; }, _ => cursor += 1 }
            }
        } else if bytes[cursor] == b'-' || bytes[cursor].is_ascii_digit() {
            let start = cursor;
            while cursor < bytes.len() && matches!(bytes[cursor], b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E') { cursor += 1; }
            let raw = std::str::from_utf8(&bytes[start..cursor]).map_err(|_| ModelContractError::Invalid)?;
            let original = normalized_decimal(raw)?;
            let number: Number = serde_json::from_str(raw).map_err(|_| ModelContractError::Invalid)?;
            let value = number.as_f64().ok_or(ModelContractError::Invalid)?;
            if !value.is_finite() || value.abs() > MAX_SAFE_INTEGER { return Err(ModelContractError::Bounds); }
            let shortest = Number::from_f64(value).ok_or(ModelContractError::Invalid)?.to_string();
            if normalized_decimal(&shortest)? != original { return Err(ModelContractError::Invalid); }
        } else { cursor += 1; }
    }
    Ok(())
}
fn normalized_decimal(raw: &str) -> Result<(bool, String, i32), ModelContractError> {
    if raw.is_empty() || raw.len() > 64 { return Err(ModelContractError::Bounds); }
    let negative = raw.starts_with('-');
    let raw = raw.strip_prefix('-').unwrap_or(raw);
    let (coefficient, exponent) = match raw.find(['e','E']) {
        Some(index) => {
            let exponent = raw[index+1..].parse::<i32>().map_err(|_| ModelContractError::Invalid)?;
            if !(-32..=32).contains(&exponent) { return Err(ModelContractError::Bounds); }
            (&raw[..index], exponent)
        },
        None => (raw, 0),
    };
    let (integer, fraction) = coefficient.split_once('.').unwrap_or((coefficient, ""));
    if integer.is_empty() || !integer.bytes().all(|byte| byte.is_ascii_digit())
        || (integer.len() > 1 && integer.starts_with('0'))
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || (coefficient.contains('.') && fraction.is_empty())
        || integer.len() + fraction.len() > 32 { return Err(ModelContractError::Invalid); }
    let digits = format!("{integer}{fraction}");
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() { return Ok((false, "0".into(), 0)); }
    let significant = digits.trim_end_matches('0');
    Ok((negative, significant.into(), exponent - fraction.len() as i32 + (digits.len()-significant.len()) as i32))
}
