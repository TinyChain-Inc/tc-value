#![forbid(unsafe_code)]

//! Canonical TinyChain values with bounded decoding and streaming.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::str::FromStr;
use std::sync::Arc;

use async_hash::{Digest, Hash, Output};
use collate::{Collate, Collator};
use get_size::GetSize;
use hr_id::Id;
use number_general::{
    ComplexType, FloatType, IntType, Number, NumberCollator, NumberInstance, UIntType,
};
use pathlink::{label, path_label, Label, Link, PathBuf, PathLabel, PathSegment};
use safecast::{CastFrom, TryCastFrom};

use crate::class::{Class, NativeClass};

pub mod class;
mod stream;

pub use class::{number_type_from_path, number_type_path};
pub use number_general::NumberType;
pub use stream::{decode_typed_value, decode_value};

const VALUE_PREFIX: PathLabel = path_label(&["state", "scalar", "value"]);
const SEGMENT_BYTES: &str = "bytes";
const SEGMENT_LINK: &str = "link";
const SEGMENT_NONE: &str = "none";
const SEGMENT_STRING: &str = "string";
const SEGMENT_TUPLE: &str = "tuple";
const LABEL_BYTES: Label = label(SEGMENT_BYTES);
const LABEL_LINK: Label = label(SEGMENT_LINK);
const LABEL_NONE: Label = label(SEGMENT_NONE);
const LABEL_STRING: Label = label(SEGMENT_STRING);
const LABEL_TUPLE: Label = label(SEGMENT_TUPLE);

/// Maximum tuple nesting accepted by codecs and conversions into Value.
/// Native constructors must respect this limit too; ordinary enum operations
/// and destruction recurse through tuple children.
pub const MAX_VALUE_DEPTH: usize = 64;

/// A native scalar value. Codecs and checked conversions bound tuple nesting.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    #[default]
    None,
    Bytes(Arc<[u8]>),
    Link(Link),
    Number(Number),
    String(String),
    Tuple(Vec<Value>),
}

impl Eq for Value {}

impl Value {
    pub fn class(&self) -> ValueType {
        match self {
            Self::None => ValueType::None,
            Self::Bytes(_) => ValueType::Bytes,
            Self::Link(_) => ValueType::Link,
            Self::Number(number) => ValueType::Number(number.class()),
            Self::String(_) => ValueType::String,
            Self::Tuple(_) => ValueType::Tuple,
        }
    }

    /// Whether this value fits within the remaining tuple nesting allowance.
    /// Conversion boundaries subtract any enclosing Scalar tuples first.
    pub fn within_depth(&self, remaining: usize) -> bool {
        match self {
            Self::Tuple(items) => {
                remaining > 0 && items.iter().all(|item| item.within_depth(remaining - 1))
            }
            _ => true,
        }
    }
}

impl<D: Digest> Hash<D> for Value {
    fn hash(self) -> Output<D> {
        Hash::<D>::hash(&self)
    }
}

impl<D: Digest> Hash<D> for &Value {
    fn hash(self) -> Output<D> {
        match self {
            Value::None => async_hash::default_hash::<D>(),
            Value::Bytes(bytes) => D::digest(bytes),
            Value::Link(link) => Hash::<D>::hash(link),
            Value::Number(number) => Hash::<D>::hash(*number),
            Value::String(string) => Hash::<D>::hash(string),
            Value::Tuple(items) if items.is_empty() => async_hash::default_hash::<D>(),
            Value::Tuple(items) => {
                let mut digest = D::new();
                for item in items {
                    digest.update(Hash::<D>::hash(item));
                }
                digest.finalize()
            }
        }
    }
}

impl GetSize for Value {
    fn get_heap_size(&self) -> usize {
        match self {
            Self::Tuple(items) => {
                items.capacity() * std::mem::size_of::<Value>()
                    + items.iter().map(GetSize::get_heap_size).sum::<usize>()
            }
            Self::Bytes(bytes) => bytes.len() + 2 * std::mem::size_of::<usize>(),
            Self::Link(link) => link.get_heap_size(),
            Self::String(string) => string.capacity(),
            _ => 0,
        }
    }
}

/// Defines the canonical collation order for [`Value`].
#[derive(Copy, Clone, Default, Eq, PartialEq)]
pub struct ValueCollator {
    bytes: Collator<Arc<[u8]>>,
    link: Collator<Link>,
    number: NumberCollator,
    string: Collator<String>,
}

impl Collate for ValueCollator {
    type Value = Value;

    fn cmp(&self, left: &Value, right: &Value) -> Ordering {
        match (left, right) {
            (Value::None, Value::None) => Ordering::Equal,
            (Value::Bytes(left), Value::Bytes(right)) => self.bytes.cmp(left, right),
            (Value::Link(left), Value::Link(right)) => self.link.cmp(left, right),
            (Value::Number(left), Value::Number(right)) => self.number.cmp(left, right),
            (Value::String(left), Value::String(right)) => self.string.cmp(left, right),
            (Value::Tuple(left), Value::Tuple(right)) => left
                .iter()
                .zip(right)
                .map(|(left, right)| self.cmp(left, right))
                .find(|order| *order != Ordering::Equal)
                .unwrap_or_else(|| left.len().cmp(&right.len())),
            _ => left.class().cmp(&right.class()),
        }
    }
}

macro_rules! value_from {
    ($($ty:ty => $kind:ident),* $(,)?) => {$ (
        impl From<$ty> for Value {
            fn from(value: $ty) -> Self {
                Value::$kind(value)
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

macro_rules! value_number {
    ($($ty:ty),+ $(,)?) => {$(
        impl From<$ty> for Value {
            fn from(value: $ty) -> Self {
                Number::from(value).into()
            }
        }
    )+};
}

value_number!(bool, i8, i16, i32, i64, u8, u16, u32, u64, f32, f64);

impl From<Vec<u8>> for Value {
    fn from(value: Vec<u8>) -> Self {
        Value::Bytes(value.into())
    }
}

impl From<bytes::Bytes> for Value {
    fn from(value: bytes::Bytes) -> Self {
        Self::Bytes(Arc::from(value.as_ref()))
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

/// Value type paths. Declaration order defines canonical cross-kind collation.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ValueType {
    None,
    Bytes,
    Number(NumberType),
    String,
    Link,
    Tuple,
}

impl ValueType {
    /// Whether this declaration accepts an actual type without converting its value.
    /// Generic numerical declarations accept their family; concrete types match exactly.
    pub fn accepts(&self, actual: &Self) -> bool {
        match (self, actual) {
            (Self::Number(NumberType::Number), Self::Number(_)) => true,
            (
                Self::Number(NumberType::Complex(ComplexType::Complex)),
                Self::Number(NumberType::Complex(_)),
            )
            | (
                Self::Number(NumberType::Float(FloatType::Float)),
                Self::Number(NumberType::Float(_)),
            )
            | (Self::Number(NumberType::Int(IntType::Int)), Self::Number(NumberType::Int(_)))
            | (Self::Number(NumberType::UInt(UIntType::UInt)), Self::Number(NumberType::UInt(_))) => {
                true
            }
            _ => self == actual,
        }
    }

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
        if let Some(dtype) = number_type_from_path(path) {
            return Some(Self::Number(dtype));
        }

        let segment = Self::from_suffix(path)?;

        match segment.as_str() {
            SEGMENT_BYTES => Some(ValueType::Bytes),
            SEGMENT_LINK => Some(ValueType::Link),
            SEGMENT_NONE => Some(ValueType::None),
            SEGMENT_STRING => Some(ValueType::String),
            SEGMENT_TUPLE => Some(ValueType::Tuple),
            _ => None,
        }
    }

    fn path(&self) -> PathBuf {
        let suffix = match self {
            Self::Number(dtype) => return number_type_path(dtype),
            Self::Bytes => LABEL_BYTES,
            Self::Link => LABEL_LINK,
            Self::None => LABEL_NONE,
            Self::String => LABEL_STRING,
            Self::Tuple => LABEL_TUPLE,
        };

        PathBuf::from(VALUE_PREFIX).append(suffix)
    }
}

// ─── safecast impls for request parsing ────────────────────────────────

// Inspection recognizes only the embedded variant; it never coerces or clones payloads.
impl TryCastFrom<&Value> for Number {
    fn can_cast_from(value: &&Value) -> bool {
        Self::opt_cast_from(*value).is_some()
    }

    fn opt_cast_from(value: &Value) -> Option<Self> {
        match value {
            Value::Number(number) => Some(*number),
            _ => None,
        }
    }
}

impl TryCastFrom<Value> for Number {
    fn can_cast_from(value: &Value) -> bool {
        matches!(value, Value::Number(_))
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        Self::opt_cast_from(&value)
    }
}

impl TryCastFrom<Value> for Link {
    fn can_cast_from(value: &Value) -> bool {
        match value {
            Value::Link(_) => true,
            Value::String(string) => string.parse::<Self>().is_ok(),
            _ => false,
        }
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value {
            Value::Link(link) => Some(link),
            Value::String(string) => string.parse().ok(),
            _ => None,
        }
    }
}

impl TryCastFrom<Value> for PathBuf {
    fn can_cast_from(value: &Value) -> bool {
        match value {
            Value::Link(link) => link.host().is_none(),
            Value::String(string) => string.parse::<Self>().is_ok(),
            _ => false,
        }
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value {
            Value::Link(link) if link.host().is_none() => Some(link.into_path()),
            Value::String(string) => string.parse().ok(),
            _ => None,
        }
    }
}

impl<'a> TryCastFrom<&'a Value> for &'a str {
    fn can_cast_from(value: &&'a Value) -> bool {
        Self::opt_cast_from(*value).is_some()
    }

    fn opt_cast_from(value: &'a Value) -> Option<Self> {
        match value {
            Value::String(string) => Some(string),
            _ => None,
        }
    }
}

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
            value => Some(vec![T::opt_cast_from(value)?]),
        }
    }
}

/// Convert scalar text, moving existing strings and formatting numbers or links.
/// Bytes, tuples, and None have no string conversion.
impl TryCastFrom<Value> for String {
    fn can_cast_from(value: &Value) -> bool {
        matches!(value, Value::String(_) | Value::Number(_) | Value::Link(_))
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value {
            Value::String(string) => Some(string),
            Value::Number(number) => Some(number.to_string()),
            Value::Link(link) => Some(link.to_string()),
            _ => None,
        }
    }
}

/// Borrow existing strings during scoped conversion; formatted values are owned.
impl<'a> TryCastFrom<&'a Value> for Cow<'a, str> {
    fn can_cast_from(value: &&'a Value) -> bool {
        String::can_cast_from(*value)
    }

    fn opt_cast_from(value: &'a Value) -> Option<Self> {
        match value {
            Value::String(string) => Some(Cow::Borrowed(string)),
            Value::Number(number) => Some(Cow::Owned(number.to_string())),
            Value::Link(link) => Some(Cow::Owned(link.to_string())),
            _ => None,
        }
    }
}

impl TryCastFrom<Value> for Arc<[u8]> {
    fn can_cast_from(value: &Value) -> bool {
        matches!(value, Value::Bytes(_))
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value {
            Value::Bytes(bytes) => Some(bytes),
            _ => None,
        }
    }
}

impl TryCastFrom<Value> for bytes::Bytes {
    fn can_cast_from(value: &Value) -> bool {
        matches!(value, Value::Bytes(_))
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value {
            Value::Bytes(bytes) => Some(Self::from_owner(bytes)),
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

macro_rules! value_tuple {
    ($length:literal; $($ty:ident: $index:tt),+ $(,)?) => {
        impl<$($ty: TryCastFrom<Value>),+> TryCastFrom<Value> for ($($ty,)+) {
            fn can_cast_from(value: &Value) -> bool {
                match value {
                    Value::Tuple(items) => {
                        items.len() == $length $(&& $ty::can_cast_from(&items[$index]))+
                    }
                    _ => false,
                }
            }

            fn opt_cast_from(value: Value) -> Option<Self> {
                let Value::Tuple(items) = value else {
                    return None;
                };

                if items.len() != $length {
                    return None;
                }

                let mut items = items.into_iter();
                Some(($($ty::opt_cast_from(items.next()?)?,)+))
            }
        }
    };
}

value_tuple!(1; T1: 0);
value_tuple!(2; T1: 0, T2: 1);
value_tuple!(3; T1: 0, T2: 1, T3: 2);

impl TryCastFrom<Value> for bool {
    fn can_cast_from(value: &Value) -> bool {
        matches!(value, Value::Number(_) | Value::None)
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value {
            Value::Number(number) => Some(bool::cast_from(number)),
            Value::None => Some(false),
            _ => None,
        }
    }
}

impl TryCastFrom<Value> for std::ops::Bound<Value> {
    fn can_cast_from(value: &Value) -> bool {
        match value {
            Value::None => true,
            Value::Tuple(pair) if pair.len() == 2 => {
                matches!(&pair[0], Value::String(tag) if tag == "in" || tag == "ex")
            }
            _ => false,
        }
    }

    fn opt_cast_from(value: Value) -> Option<Self> {
        match value {
            Value::None => Some(std::ops::Bound::Unbounded),
            Value::Tuple(mut pair) if pair.len() == 2 => {
                let value = pair.pop()?;
                match pair.pop()? {
                    Value::String(tag) if tag == "in" => Some(std::ops::Bound::Included(value)),
                    Value::String(tag) if tag == "ex" => Some(std::ops::Bound::Excluded(value)),
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
        Value::from(dtype.path().to_string())
    }
}

#[cfg(test)]
#[path = "../tests/unit/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/unit/allocation_tests.rs"]
mod allocation_tests;

#[cfg(test)]
#[path = "../tests/unit/codec_tests.rs"]
mod codec_tests;
