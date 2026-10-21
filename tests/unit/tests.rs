use std::ops::Bound;

use async_hash::{Digest, Hash, Sha256};
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
        Bound::<Value>::opt_cast_from(Value::from(vec![Value::from("in"), Value::from(1_u64),])),
        Some(Bound::Included(Value::from(1_u64)))
    );
    assert_eq!(
        Bound::<Value>::opt_cast_from(Value::from(vec![Value::from("ex"), Value::from(2_u64),])),
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
    assert!(matches!(value, Value::Number(Number::UInt(_))));
    assert_eq!(
        value.class(),
        ValueType::Number(NumberType::UInt(UIntType::U64))
    );

    let types = [
        NumberType::Number,
        NumberType::Bool,
        NumberType::Complex(ComplexType::Complex),
        NumberType::Complex(ComplexType::C32),
        NumberType::Complex(ComplexType::C64),
        NumberType::Float(FloatType::Float),
        NumberType::Float(FloatType::F32),
        NumberType::Float(FloatType::F64),
        NumberType::Int(IntType::Int),
        NumberType::Int(IntType::I8),
        NumberType::Int(IntType::I16),
        NumberType::Int(IntType::I32),
        NumberType::Int(IntType::I64),
        NumberType::UInt(UIntType::UInt),
        NumberType::UInt(UIntType::U8),
        NumberType::UInt(UIntType::U16),
        NumberType::UInt(UIntType::U32),
        NumberType::UInt(UIntType::U64),
    ];
    for dtype in types {
        let class = ValueType::Number(dtype);
        assert_eq!(
            ValueType::from_path(&class.path()),
            Some(class.clone()),
            "{dtype:?}"
        );
        assert!(ValueType::Number(NumberType::Number).accepts(&class));
        assert!(!class.accepts(&ValueType::String));
        for other in types {
            let expected = dtype == other
                || matches!(
                    (dtype, other),
                    (NumberType::Number, _)
                        | (
                            NumberType::Complex(ComplexType::Complex),
                            NumberType::Complex(_)
                        )
                        | (NumberType::Float(FloatType::Float), NumberType::Float(_))
                        | (NumberType::Int(IntType::Int), NumberType::Int(_))
                        | (NumberType::UInt(UIntType::UInt), NumberType::UInt(_))
                );
            let other = ValueType::Number(other);
            assert_eq!(class.accepts(&other), expected, "{class:?}, {other:?}");
            assert_eq!(class.cmp(&other) == Ordering::Equal, class == other);
        }
    }
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
            "signed number",
            Value::from(Number::from(-7_i64)),
            Some(b"-7"),
        ),
        ("float", Value::from(Number::from(7.5_f64)), Some(b"7.5")),
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
        cases[4].1.clone(),
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

pub(super) fn nested(depth: usize, mut leaf: Value) -> Value {
    for _ in 0..depth {
        leaf = vec![leaf].into();
    }
    leaf
}

fn reference_hash(value: &Value) -> async_hash::Output<Sha256> {
    match &value {
        Value::None => async_hash::default_hash::<Sha256>(),
        Value::Bytes(bytes) => Sha256::digest(bytes),
        Value::Link(link) => Hash::<Sha256>::hash(link),
        Value::Number(number) => Hash::<Sha256>::hash(*number),
        Value::String(string) => Hash::<Sha256>::hash(string),
        Value::Tuple(values) if values.is_empty() => async_hash::default_hash::<Sha256>(),
        Value::Tuple(values) => {
            let mut digest = Sha256::new();
            for value in values {
                digest.update(reference_hash(value));
            }
            digest.finalize()
        }
    }
}

#[test]
fn value_preserves_leaf_and_tuple_semantics() {
    let values = vec![
        Value::default(),
        vec![0_u8, 255].into(),
        "/example".parse::<pathlink::Link>().unwrap().into(),
        Number::from(7_i64).into(),
        "text".into(),
        Vec::<Value>::new().into(),
        vec![Value::from(1_u64), vec![Value::from(2_u64)].into()].into(),
    ];
    let root = Value::from(values);
    assert_eq!(Hash::<Sha256>::hash(&root), reference_hash(&root));
    assert_eq!(Hash::<Sha256>::hash(root.clone()), reference_hash(&root));
    assert_eq!(root, root.clone());
    assert_ne!(nested(3, 1_u64.into()), nested(2, 1_u64.into()));
    assert_ne!(Value::from(Vec::<Value>::new()), root);
    assert_eq!(
        format!("{:?}", nested(2, Value::default())),
        "Tuple([Tuple([None])])"
    );
    let collator = ValueCollator::default();
    let cases: [(Vec<Value>, Vec<Value>, Ordering); 3] = [
        (
            vec![1_u64.into()],
            vec![1_u64.into(), 0_u64.into()],
            Ordering::Less,
        ),
        (
            vec![2_u64.into()],
            vec![1_u64.into(), 3_u64.into()],
            Ordering::Greater,
        ),
        (vec![], vec![], Ordering::Equal),
    ];
    for (left, right, expected) in cases {
        assert_eq!(
            collator.cmp(&Value::from(left), &Value::from(right)),
            expected
        );
    }
    let nan = Number::from(f64::NAN);
    assert_eq!(Value::from(nan) == Value::from(nan), nan == nan);
}
