//! Core TinyChain value representations (WIP).

use std::collections::BTreeMap;
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
const SEGMENT_BOOL: &str = "bool";
const SEGMENT_LINK: &str = "link";
const SEGMENT_MAP: &str = "map";
const SEGMENT_NONE: &str = "none";
const SEGMENT_NUMBER: &str = "number";
const SEGMENT_STRING: &str = "string";
const SEGMENT_TUPLE: &str = "tuple";
const LABEL_BOOL: Label = label(SEGMENT_BOOL);
const LABEL_LINK: Label = label(SEGMENT_LINK);
const LABEL_MAP: Label = label(SEGMENT_MAP);
const LABEL_NONE: Label = label(SEGMENT_NONE);
const LABEL_NUMBER: Label = label(SEGMENT_NUMBER);
const LABEL_STRING: Label = label(SEGMENT_STRING);
const LABEL_TUPLE: Label = label(SEGMENT_TUPLE);

/// High-level TinyChain value enumeration (stub).
#[cfg_attr(feature = "serialize", derive(Serialize, Deserialize))]
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    Bool(bool),
    #[default]
    None,
    Link(Link),
    Map(BTreeMap<String, Value>),
    Number(Number),
    String(String),
    Tuple(Vec<Value>),
}

impl Value {
    pub fn class(&self) -> ValueType {
        match self {
            Value::Bool(_) => ValueType::Bool,
            Value::None => ValueType::None,
            Value::Link(_) => ValueType::Link,
            Value::Map(_) => ValueType::Map,
            Value::Number(_) => ValueType::Number,
            Value::String(_) => ValueType::String,
            Value::Tuple(_) => ValueType::Tuple,
        }
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Bool(value)
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

impl From<Vec<Value>> for Value {
    fn from(value: Vec<Value>) -> Self {
        Value::Tuple(value)
    }
}

impl From<BTreeMap<String, Value>> for Value {
    fn from(value: BTreeMap<String, Value>) -> Self {
        Value::Map(value)
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
    Bool,
    Link,
    Map,
    None,
    Number,
    String,
    Tuple,
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
            SEGMENT_BOOL => Some(ValueType::Bool),
            SEGMENT_LINK => Some(ValueType::Link),
            SEGMENT_MAP => Some(ValueType::Map),
            SEGMENT_NONE => Some(ValueType::None),
            SEGMENT_NUMBER => Some(ValueType::Number),
            SEGMENT_STRING => Some(ValueType::String),
            SEGMENT_TUPLE => Some(ValueType::Tuple),
            _ => None,
        }
    }

    fn path(&self) -> PathBuf {
        let prefix = PathBuf::from(VALUE_PREFIX);
        match self {
            ValueType::Bool => prefix.append(LABEL_BOOL),
            ValueType::Link => prefix.append(LABEL_LINK),
            ValueType::Map => prefix.append(LABEL_MAP),
            ValueType::None => prefix.append(LABEL_NONE),
            ValueType::Number => prefix.append(LABEL_NUMBER),
            ValueType::String => prefix.append(LABEL_STRING),
            ValueType::Tuple => prefix.append(LABEL_TUPLE),
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
                Ok(Value::Bool(value))
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

            async fn visit_seq<A: de::SeqAccess>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut items = if let Some(size) = seq.size_hint() {
                    Vec::with_capacity(size)
                } else {
                    Vec::new()
                };

                while let Some(value) = seq.next_element::<Value>(()).await? {
                    items.push(value);
                }

                Ok(Value::Tuple(items))
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
                        Some(ValueType::Bool) => {
                            let value = map.next_value::<bool>(()).await?;
                            while map.next_key::<de::IgnoredAny>(()).await?.is_some() {
                                let _ = map.next_value::<de::IgnoredAny>(()).await?;
                            }

                            return Ok(Value::Bool(value));
                        }
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
                        Some(ValueType::Map) => {
                            let nested = map.next_value::<BTreeMap<String, Value>>(()).await?;
                            while map.next_key::<de::IgnoredAny>(()).await?.is_some() {
                                let _ = map.next_value::<de::IgnoredAny>(()).await?;
                            }

                            return Ok(Value::Map(nested));
                        }
                        Some(ValueType::Tuple) => {
                            let nested = map.next_value::<Vec<Value>>(()).await?;
                            while map.next_key::<de::IgnoredAny>(()).await?.is_some() {
                                let _ = map.next_value::<de::IgnoredAny>(()).await?;
                            }

                            return Ok(Value::Tuple(nested));
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

                let mut object = BTreeMap::new();
                let first_value = map.next_value::<Value>(()).await?;
                object.insert(key, first_value);

                while let Some(next_key) = map.next_key::<String>(()).await? {
                    let value = map.next_value::<Value>(()).await?;
                    object.insert(next_key, value);
                }

                Ok(Value::Map(object))
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
            Value::Bool(value) => value.into_stream(encoder),
            Value::None => encoder.encode_unit(),
            Value::Link(link) => {
                use destream::en::EncodeMap;
                let mut map = encoder.encode_map(Some(1))?;
                map.encode_entry(link.to_string(), Vec::<()>::new())?;
                map.end()
            }
            Value::Map(map) => map.into_stream(encoder),
            Value::Number(number) => number.into_stream(encoder),
            Value::String(string) => string.into_stream(encoder),
            Value::Tuple(tuple) => tuple.into_stream(encoder),
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
        let bytes = block_on(encoded.map_err(|err| err.to_string()).try_fold(
            Vec::new(),
            |mut acc, chunk| async move {
                acc.extend_from_slice(&chunk);
                Ok(acc)
            },
        ))
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
    fn decode_plain_json_bool() {
        let stream = destream_json::encode(true).expect("encode plain json bool");
        let decoded: Value = block_on(destream_json::try_decode((), stream)).expect("decode");
        assert_eq!(decoded, Value::Bool(true));
    }

    #[test]
    fn encode_bool_value_as_plain_json_bool() {
        let value = Value::Bool(true);
        let encoded = destream_json::encode(value).expect("encode bool value");
        let bytes = block_on(encoded.map_err(|err| err.to_string()).try_fold(
            Vec::new(),
            |mut acc, chunk| async move {
                acc.extend_from_slice(&chunk);
                Ok(acc)
            },
        ))
        .expect("collect encoded bool");
        assert_eq!(bytes, b"true");
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
        let bytes = block_on(encoded.map_err(|err| err.to_string()).try_fold(
            Vec::new(),
            |mut acc, chunk| async move {
                acc.extend_from_slice(&chunk);
                Ok(acc)
            },
        ))
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

    #[test]
    fn roundtrip_tuple_value() {
        let value = Value::Tuple(vec![
            Value::Bool(true),
            Value::from(7_u64),
            Value::from("x"),
        ]);
        let encoded = destream_json::encode(value.clone()).expect("encode tuple value");
        let decoded: Value =
            block_on(destream_json::try_decode((), encoded)).expect("decode tuple");
        assert_eq!(decoded, value);
    }

    #[test]
    fn roundtrip_map_value() {
        let mut map = BTreeMap::new();
        map.insert("a".to_string(), Value::Bool(true));
        map.insert("b".to_string(), Value::from(5_u64));

        let value = Value::Map(map);
        let encoded = destream_json::encode(value.clone()).expect("encode map value");
        let decoded: Value = block_on(destream_json::try_decode((), encoded)).expect("decode map");
        assert_eq!(decoded, value);
    }
}
