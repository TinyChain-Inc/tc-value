#![forbid(unsafe_code)]

//! Canonical TinyChain values with iterative ownership and streaming.

use std::cmp::Ordering;
use std::str::FromStr;
use std::sync::Arc;

use number_general::Number;
use pathlink::{label, path_label, Label, Link, PathBuf, PathLabel, PathSegment};

use crate::class::{Class, NativeClass};
mod allocation;
pub mod class;

pub use class::{number_type_from_path, number_type_path};
pub use number_general::NumberType;

const VALUE_PREFIX: PathLabel = path_label(&["state", "scalar", "value"]);
const SEGMENT_BYTES: &str = "bytes";
const SEGMENT_LINK: &str = "link";
const SEGMENT_NONE: &str = "none";
const SEGMENT_NUMBER: &str = "number";
const SEGMENT_STRING: &str = "string";
const SEGMENT_TUPLE: &str = "tuple";
const LABEL_BYTES: Label = label(SEGMENT_BYTES);
const LABEL_LINK: Label = label(SEGMENT_LINK);
const LABEL_NONE: Label = label(SEGMENT_NONE);
const LABEL_NUMBER: Label = label(SEGMENT_NUMBER);
const LABEL_STRING: Label = label(SEGMENT_STRING);
const LABEL_TUPLE: Label = label(SEGMENT_TUPLE);

mod owned;
mod stream;

pub use owned::{Value, ValueCollator, ValueKind};
pub use stream::decode_typed_value;

macro_rules! value_from {
    ($($ty:ty => $kind:ident),* $(,)?) => {$ (
        impl From<$ty> for Value {
            fn from(value: $ty) -> Self {
                ValueKind::$kind(value).into()
            }
        }
    )*};
}

value_from! {
    Arc<[u8]> => Bytes,
    Number => Number,
    Link => Link,
    String => String,
    Vec<Value> => Tuple,
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Number::from(value).into()
    }
}
impl From<u64> for Value {
    fn from(value: u64) -> Self {
        Number::from(value).into()
    }
}
impl From<Vec<u8>> for Value {
    fn from(value: Vec<u8>) -> Self {
        ValueKind::Bytes(value.into()).into()
    }
}
impl From<bytes::Bytes> for Value {
    fn from(value: bytes::Bytes) -> Self {
        value.to_vec().into()
    }
}
impl From<&str> for Value {
    fn from(value: &str) -> Self {
        value.to_owned().into()
    }
}
impl From<()> for Value {
    fn from(_: ()) -> Self {
        Self::default()
    }
}

/// Value type paths (URI-based type declarations).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueType {
    Bytes,
    Link,
    None,
    Number,
    String,
    Tuple,
}

impl Ord for ValueType {
    fn cmp(&self, other: &Self) -> Ordering {
        fn rank(value_type: &ValueType) -> u8 {
            match value_type {
                ValueType::None => 0,
                ValueType::Bytes => 1,
                ValueType::Number => 2,
                ValueType::String => 3,
                ValueType::Link => 4,
                ValueType::Tuple => 5,
            }
        }

        rank(self).cmp(&rank(other))
    }
}

impl PartialOrd for ValueType {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
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
            SEGMENT_BYTES => Some(ValueType::Bytes),
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
            ValueType::Bytes => prefix.append(LABEL_BYTES),
            ValueType::Link => prefix.append(LABEL_LINK),
            ValueType::None => prefix.append(LABEL_NONE),
            ValueType::Number => prefix.append(LABEL_NUMBER),
            ValueType::String => prefix.append(LABEL_STRING),
            ValueType::Tuple => prefix.append(LABEL_TUPLE),
        }
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
        match value.kind() {
            ValueKind::Tuple(tuple) => tuple.iter().all(T::can_cast_from),
            _ => T::can_cast_from(value),
        }
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value.into_kind() {
            ValueKind::Tuple(tuple) => tuple.into_iter().map(T::opt_cast_from).collect(),
            kind => Some(vec![T::opt_cast_from(kind.into())?]),
        }
    }
}

impl TryCastFrom<Value> for String {
    fn can_cast_from(value: &Value) -> bool {
        matches!(value.kind(), ValueKind::String(_))
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value.into_kind() {
            ValueKind::String(s) => Some(s),
            _ => None,
        }
    }
}

impl TryCastFrom<Value> for Arc<[u8]> {
    fn can_cast_from(value: &Value) -> bool {
        matches!(value.kind(), ValueKind::Bytes(_))
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value.into_kind() {
            ValueKind::Bytes(bytes) => Some(bytes),
            _ => None,
        }
    }
}

impl TryCastFrom<Value> for bytes::Bytes {
    fn can_cast_from(value: &Value) -> bool {
        matches!(value.kind(), ValueKind::Bytes(_))
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value.into_kind() {
            ValueKind::Bytes(bytes) => Some(Self::from_owner(bytes)),
            _ => None,
        }
    }
}

impl TryCastFrom<Value> for Id {
    fn can_cast_from(value: &Value) -> bool {
        match value.kind() {
            ValueKind::String(s) => <Id as TryCastFrom<String>>::can_cast_from(s),
            _ => false,
        }
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value.into_kind() {
            ValueKind::String(s) => Id::opt_cast_from(s),
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
        let ValueKind::Tuple(pair) = value.kind() else {
            return false;
        };
        pair.len() == 2 && T1::can_cast_from(&pair[0]) && T2::can_cast_from(&pair[1])
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        let ValueKind::Tuple(pair) = value.into_kind() else {
            return None;
        };
        if pair.len() != 2 {
            return None;
        }
        let mut pair = pair.into_iter();
        Some((
            T1::opt_cast_from(pair.next()?)?,
            T2::opt_cast_from(pair.next()?)?,
        ))
    }
}

impl TryCastFrom<Value> for bool {
    fn can_cast_from(value: &Value) -> bool {
        matches!(value.kind(), ValueKind::Number(_) | ValueKind::None)
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value.into_kind() {
            ValueKind::Number(n) => {
                use safecast::CastFrom;
                Some(u64::cast_from(n) != 0)
            }
            ValueKind::None => Some(false),
            _ => None,
        }
    }
}

impl TryCastFrom<Value> for std::ops::Bound<Value> {
    fn can_cast_from(value: &Value) -> bool {
        Self::opt_cast_from(value.clone()).is_some()
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value.into_kind() {
            ValueKind::None => Some(std::ops::Bound::Unbounded),
            ValueKind::Tuple(mut pair) if pair.len() == 2 => {
                let value = pair.pop()?;
                match pair.pop()?.into_kind() {
                    ValueKind::String(tag) if tag == "in" => Some(std::ops::Bound::Included(value)),
                    ValueKind::String(tag) if tag == "ex" => Some(std::ops::Bound::Excluded(value)),
                    _ => None,
                }
            }
            _ => None,
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
        match value.kind() {
            ValueKind::String(s) => {
                let path = pathlink::PathBuf::from_str(s).ok()?;
                NativeClass::from_path(&path)
            }
            _ => None,
        }
    }
}

impl safecast::CastFrom<ValueType> for Value {
    fn cast_from(dtype: ValueType) -> Self {
        Value::from(dtype.path().to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Bound;

    use collate::Collate;
    use destream::{de, en};
    use futures::TryStreamExt;

    use super::*;

    #[test]
    fn cast_tagged_bounds() {
        assert_eq!(
            Bound::<Value>::opt_cast_from(Value::default()),
            Some(Bound::Unbounded)
        );
        assert_eq!(
            Bound::<Value>::opt_cast_from(Value::from(
                vec![Value::from("in"), Value::from(1_u64),]
            )),
            Some(Bound::Included(Value::from(1_u64)))
        );
        assert_eq!(
            Bound::<Value>::opt_cast_from(Value::from(
                vec![Value::from("ex"), Value::from(2_u64),]
            )),
            Some(Bound::Excluded(Value::from(2_u64)))
        );
    }

    #[test]
    fn reject_untagged_bounds() {
        assert_eq!(Bound::<Value>::opt_cast_from(Value::from(1_u64)), None);
        assert_eq!(
            Bound::<Value>::opt_cast_from(Value::from(vec![
                Value::from("unknown"),
                Value::from(1_u64),
            ])),
            None
        );
    }

    async fn encode_json_bytes<'en, T>(value: T) -> Vec<u8>
    where
        T: en::IntoStream<'en> + 'en,
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
        assert!(matches!(value.kind(), ValueKind::Number(Number::UInt(_))));
    }

    #[test]
    fn value_collator_delegates_numeric_ordering() {
        let collator = ValueCollator::default();

        assert_eq!(
            collator.cmp(&Value::from(9_000_u64), &Value::from(11_000_u64)),
            Ordering::Less
        );

        let integer = Value::from(Number::from(9_i64));
        let float = Value::from(Number::from(9.5_f64));
        assert_eq!(collator.cmp(&integer, &float), Ordering::Less);
    }

    #[test]
    fn value_collator_delegates_complex_ordering() {
        let collator = ValueCollator::default();
        let five = Value::from(Number::from(number_general::Complex::from([3_f32, 4_f32])));
        let ten = Value::from(Number::from(number_general::Complex::from([6_f32, 8_f32])));

        assert_eq!(collator.cmp(&five, &ten), Ordering::Less);
    }

    #[test]
    fn value_collator_recurses_through_tuples() {
        let collator = ValueCollator::default();
        let left = Value::from(vec![Value::from("same"), Value::from(9_000_u64)]);
        let right = Value::from(vec![Value::from("same"), Value::from(11_000_u64)]);

        assert_eq!(collator.cmp(&left, &right), Ordering::Less);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn json_value_cases() {
        let cases: Vec<(&str, Value, Option<&[u8]>)> = vec![
            ("number", Value::from(42_u64), Some(b"42")),
            ("literal number", Value::from(7_u64), Some(b"7")),
            (
                "bool",
                Value::from(Number::Bool(true.into())),
                Some(b"true"),
            ),
            ("string", Value::from("hello"), Some(br#""hello""#)),
            ("bytes", Value::from(Arc::from([0_u8, 1, 2, 255])), None),
            (
                "link",
                Value::from(
                    Link::from_str(
                        &PathBuf::from(path_label(&["state", "scalar", "ref", "cond"])).to_string(),
                    )
                    .unwrap(),
                ),
                None,
            ),
            (
                "tuple",
                Value::from(vec![
                    Value::from(true),
                    Value::from(7_u64),
                    Value::from("x"),
                ]),
                None,
            ),
        ];
        for (name, value, literal) in &cases {
            let encoded = encode_json_bytes(value).await;
            assert_eq!(
                encoded,
                encode_json_bytes(value.clone()).await,
                "{name}: ownership"
            );
            if let Some(literal) = literal {
                assert_eq!(encoded, *literal, "{name}: wire");
                let decoded: Value = decode_json_value(futures::stream::iter([Ok(
                    bytes::Bytes::copy_from_slice(literal),
                )]))
                .await;
                assert_eq!(&decoded, value, "{name}: literal decoding");
            }
            let decoded: Value =
                decode_json_value(futures::stream::iter([Ok(bytes::Bytes::from(encoded))])).await;
            assert_eq!(&decoded, value, "{name}: round trip");
        }

        // Preserve the original aggregate parity fixture, including its large child.
        let values = vec![
            Value::default(),
            Value::from(Arc::from([0_u8, 1, 255])),
            Value::from("/example".parse::<Link>().unwrap()),
            cases[2].1.clone(),
            cases[0].1.clone(),
            Value::from("quoted \"value\""),
            Value::from(Vec::<Value>::new()),
            Value::from(vec![Value::from("x".repeat(200_000))]),
        ];
        for value in values
            .iter()
            .chain(std::iter::once(&Value::from(values.clone())))
        {
            assert_eq!(
                encode_json_bytes(value).await,
                encode_json_bytes(value.clone()).await
            );
        }
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

#[cfg(test)]
mod allocation_tests {
    use super::*;
    use get_size::GetSize;

    #[test]
    fn retained_size_includes_inline_values_and_spare_nested_capacity() {
        let mut string = String::with_capacity(64);
        string.push_str("abc");
        let string_capacity = string.capacity();
        let mut values = Vec::with_capacity(8);
        values.push(Value::default());
        values.push(Value::from(string));
        let capacity = values.capacity();
        assert_eq!(Value::default().get_size(), std::mem::size_of::<Value>());
        assert_eq!(
            Value::from(values).get_size(),
            std::mem::size_of::<Value>()
                + capacity * std::mem::size_of::<Value>()
                + string_capacity
        );
        let link: Link = "https://example.com/one/two/three".parse().unwrap();
        assert!(link.get_heap_size() > 0);
        assert!(Value::from(link).get_size() > std::mem::size_of::<Value>());
    }
}

#[cfg(test)]
mod lifecycle_tests;
