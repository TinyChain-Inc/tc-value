//! Core TinyChain value representations (WIP).

use std::cmp::Ordering;
use std::str::FromStr;

use crate::class::{Class, NativeClass};
use destream::{de, en, IntoStream};
use number_general::{Number, NumberInstance};
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
const SEGMENT_TUPLE: &str = "tuple";
const LABEL_LINK: Label = label(SEGMENT_LINK);
const LABEL_NONE: Label = label(SEGMENT_NONE);
const LABEL_NUMBER: Label = label(SEGMENT_NUMBER);
const LABEL_STRING: Label = label(SEGMENT_STRING);
const LABEL_TUPLE: Label = label(SEGMENT_TUPLE);

/// High-level TinyChain value enumeration (stub).
#[cfg_attr(feature = "serialize", derive(Serialize, Deserialize))]
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    #[default]
    None,
    Link(Link),
    Number(Number),
    String(String),
    Tuple(Vec<Value>),
}

impl Eq for Value {}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Value {
    fn cmp(&self, other: &Self) -> Ordering {
        fn tag(value: &Value) -> u8 {
            match value {
                Value::None => 0,
                Value::Number(_) => 1,
                Value::String(_) => 2,
                Value::Link(_) => 3,
                Value::Tuple(_) => 4,
            }
        }

        let tag_ord = tag(self).cmp(&tag(other));
        if tag_ord != Ordering::Equal {
            return tag_ord;
        }

        match (self, other) {
            (Value::None, Value::None) => Ordering::Equal,
            (Value::Number(left), Value::Number(right)) if left == right => Ordering::Equal,
            (Value::Number(left), Value::Number(right)) => {
                let class_ord = left.class().cmp(&right.class());
                if class_ord != Ordering::Equal {
                    return class_ord;
                }

                // Use a stable textual ordering for non-equal numeric values to guarantee a total order.
                left.to_string().cmp(&right.to_string())
            }
            (Value::String(left), Value::String(right)) => left.cmp(right),
            (Value::Link(left), Value::Link(right)) => left.to_string().cmp(&right.to_string()),
            (Value::Tuple(left), Value::Tuple(right)) => left.cmp(right),
            _ => unreachable!("Value tag ordering must align with variant matching"),
        }
    }
}

impl Value {
    pub fn class(&self) -> ValueType {
        match self {
            Value::None => ValueType::None,
            Value::Link(_) => ValueType::Link,
            Value::Number(_) => ValueType::Number,
            Value::String(_) => ValueType::String,
            Value::Tuple(_) => ValueType::Tuple,
        }
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Number(Number::from(value))
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
            SEGMENT_LINK => Some(ValueType::Link),
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
            ValueType::Link => prefix.append(LABEL_LINK),
            ValueType::None => prefix.append(LABEL_NONE),
            ValueType::Number => prefix.append(LABEL_NUMBER),
            ValueType::String => prefix.append(LABEL_STRING),
            ValueType::Tuple => prefix.append(LABEL_TUPLE),
        }
    }
}

/// Decode a typed `/state/scalar/value/...` map entry if `key` is a recognized Value type path.
///
/// Returns `Ok(Some(Value))` when `key` is a typed Value path and the corresponding value was
/// decoded from `map`; otherwise returns `Ok(None)` and leaves the caller to handle the entry.
pub async fn decode_typed_value_map_entry<A: de::MapAccess>(
    key: &str,
    map: &mut A,
) -> Result<Option<Value>, A::Error> {
    let Ok(path) = key.parse::<PathBuf>() else {
        return Ok(None);
    };

    let Some(value_type) = ValueType::from_path(&path) else {
        let prefix_len = VALUE_PREFIX[..].len();
        if path.len() == prefix_len + 1 && path[..prefix_len] == VALUE_PREFIX[..] {
            return Err(de::Error::custom(format!(
                "unsupported value type path {path}"
            )));
        }

        return Ok(None);
    };

    let value = match value_type {
        ValueType::Number => {
            let number = map.next_value::<Number>(()).await?;
            Value::Number(number)
        }
        ValueType::None => {
            let _ = map.next_value::<de::IgnoredAny>(()).await?;
            Value::None
        }
        ValueType::String => {
            let string = map.next_value::<String>(()).await?;
            Value::String(string)
        }
        ValueType::Link => {
            let link_raw = map.next_value::<String>(()).await?;
            let link =
                Link::from_str(&link_raw).map_err(|err| de::Error::custom(err.to_string()))?;
            Value::Link(link)
        }
        ValueType::Tuple => {
            let nested = map.next_value::<Vec<Value>>(()).await?;
            Value::Tuple(nested)
        }
    };

    // Drain trailing entries to keep decoder state in sync with tolerant v1 map semantics.
    while map.next_key::<de::IgnoredAny>(()).await?.is_some() {
        let _ = map.next_value::<de::IgnoredAny>(()).await?;
    }

    Ok(Some(value))
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

                if let Some(value) = decode_typed_value_map_entry(&key, &mut map).await? {
                    return Ok(value);
                }

                if let Ok(link) = Link::from_str(&key) {
                    let _ = map.next_value::<de::IgnoredAny>(()).await?;
                    while map.next_key::<de::IgnoredAny>(()).await?.is_some() {
                        let _ = map.next_value::<de::IgnoredAny>(()).await?;
                    }
                    return Ok(Value::Link(link));
                }

                Err(de::Error::custom("map values are not supported"))
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
            Value::Tuple(tuple) => tuple.into_stream(encoder),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::TryStreamExt;

    async fn encode_json_bytes<T>(value: T) -> Vec<u8>
    where
        T: for<'en> en::IntoStream<'en>,
    {
        destream_json::encode(value)
            .expect("encode json value")
            .map_err(|err| err.to_string())
            .try_fold(Vec::new(), |mut acc, chunk| async move {
                acc.extend_from_slice(&chunk);
                Ok(acc)
            })
            .await
            .expect("collect encoded value")
    }

    async fn decode_json_value<T>(
        stream: impl futures::Stream<Item = Result<bytes::Bytes, String>> + Send + Unpin,
    ) -> T
    where
        T: de::FromStream<Context = ()>,
    {
        destream_json::try_decode((), stream)
            .await
            .expect("decode value")
    }

    #[test]
    fn value_from_u64() {
        let value = Value::from(123_u64);
        assert!(matches!(value, Value::Number(Number::UInt(_))));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn roundtrip_json_number_value() {
        let value = Value::from(42_u64);
        let encoded = destream_json::encode(value.clone()).expect("encode number value");
        let decoded: Value = decode_json_value(encoded.map_err(|err| err.to_string())).await;
        assert_eq!(decoded, value);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn encode_number_value_as_plain_json_number() {
        let value = Value::from(42_u64);
        let bytes = encode_json_bytes(value).await;
        assert_eq!(bytes, b"42");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn decode_plain_json_number() {
        let stream = destream_json::encode(7_u64).expect("encode plain json number");
        let decoded: Value = decode_json_value(stream.map_err(|err| err.to_string())).await;
        assert_eq!(decoded, Value::from(7_u64));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn decode_plain_json_bool() {
        let stream = destream_json::encode(true).expect("encode plain json bool");
        let decoded: Value = decode_json_value(stream.map_err(|err| err.to_string())).await;
        assert_eq!(decoded, Value::Number(Number::Bool(true.into())));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn encode_bool_value_as_plain_json_bool() {
        let value = Value::Number(Number::Bool(true.into()));
        let bytes = encode_json_bytes(value).await;
        assert_eq!(bytes, b"true");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn roundtrip_string_value() {
        let value = Value::from("hello");
        let encoded = destream_json::encode(value.clone()).expect("encode string value");
        let decoded: Value = decode_json_value(encoded.map_err(|err| err.to_string())).await;
        assert_eq!(decoded, value);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn encode_string_value_as_plain_json_string() {
        let value = Value::from("hello");
        let bytes = encode_json_bytes(value).await;
        assert_eq!(bytes, br#""hello""#);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn roundtrip_link_value() {
        let link = Link::from_str(
            &PathBuf::from(path_label(&["state", "scalar", "ref", "if"])).to_string(),
        )
        .expect("link");
        let value = Value::from(link);
        let encoded = destream_json::encode(value.clone()).expect("encode link value");
        let decoded: Value = decode_json_value(encoded.map_err(|err| err.to_string())).await;
        assert_eq!(decoded, value);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn roundtrip_tuple_value() {
        let value = Value::Tuple(vec![
            Value::Number(Number::Bool(true.into())),
            Value::from(7_u64),
            Value::from("x"),
        ]);
        let encoded = destream_json::encode(value.clone()).expect("encode tuple value");
        let decoded: Value = decode_json_value(encoded.map_err(|err| err.to_string())).await;
        assert_eq!(decoded, value);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn decode_plain_json_map_fails_closed() {
        let stream =
            destream_json::encode(std::collections::BTreeMap::from([("a".to_string(), 1_u64)]))
                .expect("encode plain json map");

        let decoded: Result<Value, _> = destream_json::try_decode((), stream).await;
        assert!(decoded.is_err());
    }
}

// ─── safecast impls for request parsing ────────────────────────────────

use hr_id::Id;
use safecast::TryCastFrom;

impl<T> TryCastFrom<Value> for Vec<T>
where
    T: TryCastFrom<Value>,
{
    fn can_cast_from(value: &Value) -> bool {
        match value {
            Value::Tuple(tuple) => tuple.iter().all(T::can_cast_from),
            _ => T::can_cast_from(value),
        }
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value {
            Value::Tuple(tuple) => tuple.into_iter().map(T::opt_cast_from).collect(),
            _ => Some(vec![T::opt_cast_from(value)?]),
        }
    }
}

impl TryCastFrom<Value> for String {
    fn can_cast_from(value: &Value) -> bool {
        matches!(value, Value::String(_))
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
}

impl TryCastFrom<Value> for Id {
    fn can_cast_from(value: &Value) -> bool {
        match value {
            Value::String(s) => <Id as TryCastFrom<String>>::can_cast_from(s),
            _ => false,
        }
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value {
            Value::String(s) => Id::opt_cast_from(s),
            _ => None,
        }
    }
}

impl<T1, T2> TryCastFrom<Value> for (T1, T2)
where
    T1: TryCastFrom<Value>,
    T2: TryCastFrom<Value>,
{
    fn can_cast_from(value: &Value) -> bool {
        let Value::Tuple(pair) = value else {
            return false;
        };
        pair.len() == 2 && T1::can_cast_from(&pair[0]) && T2::can_cast_from(&pair[1])
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        let Value::Tuple(pair) = value else {
            return None;
        };
        if pair.len() != 2 {
            return None;
        }
        Some((
            T1::opt_cast_from(pair[0].clone())?,
            T2::opt_cast_from(pair[1].clone())?,
        ))
    }
}

impl TryCastFrom<Value> for bool {
    fn can_cast_from(value: &Value) -> bool {
        matches!(value, Value::Number(_) | Value::None)
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value {
            Value::Number(n) => {
                use safecast::CastFrom;
                Some(u64::cast_from(n) != 0)
            }
            Value::None => Some(false),
            _ => None,
        }
    }
}

impl TryCastFrom<Value> for std::ops::Bound<Value> {
    fn can_cast_from(_value: &Value) -> bool {
        true
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value {
            Value::None => Some(std::ops::Bound::Unbounded),
            other => Some(std::ops::Bound::Included(other)),
        }
    }
}

impl TryCastFrom<Value> for ValueType {
    fn can_cast_from(value: &Value) -> bool {
        Self::opt_cast_from_ref(value).is_some()
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        Self::opt_cast_from_ref(&value)
    }
}

impl ValueType {
    fn opt_cast_from_ref(value: &Value) -> Option<Self> {
        match value {
            Value::String(s) => {
                let path = pathlink::PathBuf::from_str(s).ok()?;
                NativeClass::from_path(&path)
            }
            _ => None,
        }
    }
}

impl safecast::CastFrom<ValueType> for Value {
    fn cast_from(dtype: ValueType) -> Self {
        Value::String(dtype.path().to_string())
    }
}
