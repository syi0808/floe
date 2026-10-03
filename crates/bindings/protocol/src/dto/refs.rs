use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use uuid::Uuid;

/// A non-nil UUID encoded only in its canonical lowercase hyphenated form.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UuidRefDto(Uuid);

impl UuidRefDto {
    pub fn new(value: Uuid) -> Option<Self> {
        (!value.is_nil()).then_some(Self(value))
    }

    pub fn get(self) -> Uuid {
        self.0
    }
}

impl Serialize for UuidRefDto {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for UuidRefDto {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let parsed = Uuid::parse_str(&value).map_err(de::Error::custom)?;
        if parsed.is_nil() || parsed.to_string() != value {
            return Err(de::Error::custom("UUID must be non-nil and canonical"));
        }
        Ok(Self(parsed))
    }
}

macro_rules! typed_uuid_ref {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(UuidRefDto);

        impl $name {
            pub fn new(value: Uuid) -> Option<Self> {
                UuidRefDto::new(value).map(Self)
            }

            pub fn get(self) -> Uuid {
                self.0.get()
            }
        }
    };
}

typed_uuid_ref!(RequestIdDto);
typed_uuid_ref!(CommandIdDto);
typed_uuid_ref!(SessionRefDto);
typed_uuid_ref!(RunRefDto);
typed_uuid_ref!(MessageRefDto);
typed_uuid_ref!(InteractionRefDto);
typed_uuid_ref!(AssignmentRefDto);
typed_uuid_ref!(TaskRefDto);
typed_uuid_ref!(AttemptRefDto);
typed_uuid_ref!(ActionRefDto);
typed_uuid_ref!(GatewaySetupRefDto);
typed_uuid_ref!(GatewayRefDto);
typed_uuid_ref!(OperationRefDto);
typed_uuid_ref!(IntegrationRefDto);
typed_uuid_ref!(ConnectionsSourceRefDto);
typed_uuid_ref!(ResourceRefDto);
typed_uuid_ref!(LaunchActionRefDto);

/// Lowercase hexadecimal SHA-256 digest, represented by exactly 64 bytes.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DigestHex64Dto(String);

impl DigestHex64Dto {
    pub fn new(value: String) -> Option<Self> {
        is_hex64(&value).then_some(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for DigestHex64Dto {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for DigestHex64Dto {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if !is_hex64(&value) {
            return Err(de::Error::custom(
                "digest must be 64 lowercase hexadecimal bytes",
            ));
        }
        Ok(Self(value))
    }
}

fn is_hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRefDto {
    pub id: UuidRefDto,
    pub revision: u64,
    pub digest: DigestHex64Dto,
}

impl<'de> Deserialize<'de> for ReviewRefDto {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: UuidRefDto,
            revision: u64,
            digest: DigestHex64Dto,
        }

        let fields = Fields::deserialize(deserializer)?;
        if fields.revision == 0 || fields.revision > i64::MAX as u64 {
            return Err(de::Error::custom(
                "review revision must be in range 1..=i64::MAX",
            ));
        }
        Ok(Self {
            id: fields.id,
            revision: fields.revision,
            digest: fields.digest,
        })
    }
}

impl ReviewRefDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.revision == 0 || self.revision > i64::MAX as u64 {
            Err("review_ref.revision")
        } else {
            Ok(())
        }
    }
}
