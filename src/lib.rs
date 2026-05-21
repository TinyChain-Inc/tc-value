//! Core TinyChain value representations (WIP).

use std::str::FromStr;

use crate::class::{Class, NativeClass};
use destream::{de, en, IntoStream};
use number_general::Number;
use pathlink::{label, path_label, Label, Link, PathBuf, PathLabel, PathSegment};
#[cfg(feature = "serialize")]
use serde::{Deserialize, Serialize};
pub mod class;

pub use class::{number_type_from_path, number_type_path};
pub use number_general::NumberType;

const VALUE_PREFIX: PathLabel = path_label(&["state", "scalar", "value"]);
const SEGMENT_LINK: &str = "link";
const SEGMENT_NONE: &str = "none";
const SEGMENT_NUMBER: &str = "number";
const SEGMENT_STRING: &str = "string";
const LABEL_LINK: Label = label(SEGMENT_LINK);
const LABEL_NONE: Label = label(SEGMENT_NONE);
const LABEL_NUMBER: Label = label(SEGMENT_NUMBER);
const LABEL_STRING: Label = label(SEGMENT_STRING);

/// High-level TinyChain value enumeration (stub).
#[cfg_attr(feature = "serialize", derive(Serialize, Deserialize))]
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    #[default]
    None,
    Link(Link),
    Number(Number),
    String(String),
}

impl Value {
    pub fn class(&self) -> ValueType {
        match self {
            Value::None => ValueType::None,
            Value::Link(_) => ValueType::Link,
            Value::Number(_) => ValueType::Number,
            Value::String(_) => ValueType::String,
        }
    }
}

impl From<Number> for Value {
    fn from(n: Number) -> Self {
        Value::Number(n)
    }
}

impl From<Link> for Value {
    fn from(link: Link) -> Self {
        Value::Link(link)
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Value::String(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Value::String(value.to_string())
    }
}

impl From<u64> for Value {
    fn from(value: u64) -> Self {
        Value::Number(Number::from(value))
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Value::None
    }
}

/// Value type paths (URI-based type declarations).
#[cfg_attr(feature = "serialize", derive(Serialize, Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueType {
    Link,
    None,
    Number,
    String,
}

impl ValueType {
    fn from_suffix(path: &[PathSegment]) -> Option<&PathSegment> {
        let prefix_len = VALUE_PREFIX[..].len();
        if path.len() != prefix_len + 1 {
            return None;
        }

        if path[..prefix_len] != VALUE_PREFIX[..] {
            return None;
        }

        Some(&path[prefix_len])
    }
}

impl Class for ValueType {}

impl NativeClass for ValueType {
    fn from_path(path: &[PathSegment]) -> Option<Self> {
        let segment = Self::from_suffix(path)?;

        match segment.as_str() {
            SEGMENT_LINK => Some(ValueType::Link),
            SEGMENT_NONE => Some(ValueType::None),
            SEGMENT_NUMBER => Some(ValueType::Number),
            SEGMENT_STRING => Some(ValueType::String),
            _ => None,
        }
    }

    fn path(&self) -> PathBuf {
        let prefix = PathBuf::from(VALUE_PREFIX);
        match self {
            ValueType::Link => prefix.append(LABEL_LINK),
            ValueType::None => prefix.append(LABEL_NONE),
            ValueType::Number => prefix.append(LABEL_NUMBER),
            ValueType::String => prefix.append(LABEL_STRING),
        }
    }
}

impl de::FromStream for Value {
    type Context = ();

    async fn from_stream<D: de::Decoder>(
        _context: Self::Context,
        decoder: &mut D,
    ) -> Result<Self, D::Error> {
        struct ValueVisitor;

        impl de::Visitor for ValueVisitor {
            type Value = Value;

            fn expecting() -> &'static str {
                "a TinyChain scalar value"
            }

            fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(Value::None)
            }

            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(Value::None)
            }

            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(Value::Number(Number::from(value)))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(Value::Number(Number::from(value)))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(Value::Number(Number::from(value)))
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Ok(Value::Number(Number::from(value)))
            }

            fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(Value::String(value))
            }

            async fn visit_map<A: de::MapAccess>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let key = map
                    .next_key::<String>(())
                    .await?
                    .ok_or_else(|| de::Error::custom("expected TinyChain type path key"))?;

                if let Ok(path) = key.parse::<PathBuf>() {
                    match ValueType::from_path(&path) {
                        Some(ValueType::Number) => {
                            let number = map.next_value::<Number>(()).await?;
                            // Drain any trailing entries to keep the decoder in sync.
                            while map.next_key::<de::IgnoredAny>(()).await?.is_some() {
                                let _ = map.next_value::<de::IgnoredAny>(()).await?;
                            }

                            return Ok(Value::Number(number));
                        }
                        Some(ValueType::None) => {
                            let _ = map.next_value::<de::IgnoredAny>(()).await?;
                            return Ok(Value::None);
                        }
                        Some(ValueType::String) => {
                            let string = map.next_value::<String>(()).await?;
                            while map.next_key::<de::IgnoredAny>(()).await?.is_some() {
                                let _ = map.next_value::<de::IgnoredAny>(()).await?;
                            }

                            return Ok(Value::String(string));
                        }
                        Some(ValueType::Link) => {
                            let link_raw = map.next_value::<String>(()).await?;
                            let link = Link::from_str(&link_raw)
                                .map_err(|err| de::Error::custom(err.to_string()))?;
                            while map.next_key::<de::IgnoredAny>(()).await?.is_some() {
                                let _ = map.next_value::<de::IgnoredAny>(()).await?;
                            }

                            return Ok(Value::Link(link));
                        }
                        None => {}
                    }
                }

                if let Ok(link) = Link::from_str(&key) {
                    let _ = map.next_value::<de::IgnoredAny>(()).await?;
                    while map.next_key::<de::IgnoredAny>(()).await?.is_some() {
                        let _ = map.next_value::<de::IgnoredAny>(()).await?;
                    }
                    return Ok(Value::Link(link));
                }

                Err(de::Error::invalid_value(
                    key,
                    "a known TinyChain value type path",
                ))
            }
        }

        decoder.decode_any(ValueVisitor).await
    }
}

impl<'en> en::ToStream<'en> for Value {
    fn to_stream<E: en::Encoder<'en>>(&'en self, encoder: E) -> Result<E::Ok, E::Error> {
        self.clone().into_stream(encoder)
    }
}

impl<'en> en::IntoStream<'en> for Value {
    fn into_stream<E: en::Encoder<'en>>(self, encoder: E) -> Result<E::Ok, E::Error> {
        match self {
            Value::None => encoder.encode_unit(),
            Value::Link(link) => {
                use destream::en::EncodeMap;
                let mut map = encoder.encode_map(Some(1))?;
                map.encode_entry(link.to_string(), Vec::<()>::new())?;
                map.end()
            }
            Value::Number(number) => number.into_stream(encoder),
            Value::String(string) => string.into_stream(encoder),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{executor::block_on, TryStreamExt};

    #[test]
    fn value_from_u64() {
        let value = Value::from(123_u64);
        assert!(matches!(value, Value::Number(Number::UInt(_))));
    }

    #[test]
    fn roundtrip_json_number_value() {
        let value = Value::from(42_u64);
        let encoded = destream_json::encode(value.clone()).expect("encode number value");
        let decoded: Value =
            block_on(destream_json::try_decode((), encoded)).expect("decode number");
        assert_eq!(decoded, value);
    }

    #[test]
    fn encode_number_value_as_plain_json_number() {
        let value = Value::from(42_u64);
        let encoded = destream_json::encode(value).expect("encode number value");
        let bytes = block_on(
            encoded
                .map_err(|err| err.to_string())
                .try_fold(Vec::new(), |mut acc, chunk| async move {
                    acc.extend_from_slice(&chunk);
                    Ok(acc)
                }),
        )
        .expect("collect encoded number");
        assert_eq!(bytes, b"42");
    }

    #[test]
    fn decode_plain_json_number() {
        let stream = destream_json::encode(7_u64).expect("encode plain json number");
        let decoded: Value = block_on(destream_json::try_decode((), stream)).expect("decode");
        assert_eq!(decoded, Value::from(7_u64));
    }

    #[test]
    fn roundtrip_string_value() {
        let value = Value::from("hello");
        let encoded = destream_json::encode(value.clone()).expect("encode string value");
        let decoded: Value =
            block_on(destream_json::try_decode((), encoded)).expect("decode string");
        assert_eq!(decoded, value);
    }

    #[test]
    fn encode_string_value_as_plain_json_string() {
        let value = Value::from("hello");
        let encoded = destream_json::encode(value).expect("encode string value");
        let bytes = block_on(
            encoded
                .map_err(|err| err.to_string())
                .try_fold(Vec::new(), |mut acc, chunk| async move {
                    acc.extend_from_slice(&chunk);
                    Ok(acc)
                }),
        )
        .expect("collect encoded string");
        assert_eq!(bytes, br#""hello""#);
    }

    #[test]
    fn roundtrip_link_value() {
        let link = Link::from_str(
            &PathBuf::from(path_label(&["state", "scalar", "ref", "if"])).to_string(),
        )
        .expect("link");
        let value = Value::from(link);
        let encoded = destream_json::encode(value.clone()).expect("encode link value");
        let decoded: Value = block_on(destream_json::try_decode((), encoded)).expect("decode link");
        assert_eq!(decoded, value);
    }
}
