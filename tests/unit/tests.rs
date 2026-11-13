use std::ops::Bound;

use async_hash::{Digest, Hash, Sha256};
use collate::Collate;
use destream::{de, en};
use futures::TryStreamExt;

use super::*;

#[test]
fn cast_tagged_bounds() {
    for input in [
        Value::None,
        Value::from(vec![Value::from("in"), Value::from(1_u64)]),
        Value::from(vec![Value::from("ex"), Value::from("payload")]),
        Value::from(vec![Value::from("bad"), Value::None]),
        Value::from(vec![Value::from("in")]),
        Value::from(1_u64),
    ] {
        assert_eq!(
            Bound::<Value>::can_cast_from(&input),
            Bound::<Value>::opt_cast_from(input.clone()).is_some()
        );
    }

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

#[test]
fn borrowed_inspection_and_leaf_operations() {
    let number = Value::from(7u64);
    let string = Value::from("text");
    assert_eq!(Number::opt_cast_from(&number), Some(Number::from(7u64)));
    assert!(Number::can_cast_from(&&number));
    assert_eq!(<&str>::opt_cast_from(&number), None);
    assert_eq!(Number::opt_cast_from(&Value::from("7")), None);
    let borrowed = <&str>::opt_cast_from(&string).unwrap();
    assert_eq!(borrowed, "text");
    assert!(<&str>::can_cast_from(&&string));
    let Cow::Borrowed(text) = Cow::<str>::opt_cast_from(&string).unwrap() else {
        panic!("string rendering must borrow")
    };

    assert!(std::ptr::eq(text.as_ptr(), borrowed.as_ptr()));
    assert!(Cow::<str>::can_cast_from(&&string));
    let owned = String::from("owned text");
    let pointer = owned.as_ptr();
    let converted = String::opt_cast_from(Value::String(owned)).unwrap();
    assert_eq!(converted, "owned text");
    assert_eq!(converted.as_ptr(), pointer);
    assert_eq!(Cow::<str>::opt_cast_from(&number).unwrap(), "7");
    assert_eq!(String::opt_cast_from(number.clone()).unwrap(), "7");
    assert!(String::can_cast_from(&number));

    let path = PathBuf::from(path_label(&["example"]));
    let link = Value::from(Link::from_str(&path.to_string()).unwrap());
    let spelling = Value::from(path.to_string());
    assert_ne!(spelling, link);
    assert_ne!(link, spelling);
    assert_ne!(link, string);
    assert_eq!(Cow::<str>::opt_cast_from(&link).unwrap(), path.to_string());
    assert_eq!(
        String::opt_cast_from(link.clone()).unwrap(),
        path.to_string()
    );

    assert!(String::can_cast_from(&link));
    assert!(Cow::<str>::can_cast_from(&&link));
    assert_eq!(number, Value::Number(Number::from(7f64)));
    assert_ne!(number, Value::from("7"));
    assert_eq!(string, Value::from("text"));
    assert_ne!(
        Value::Tuple(vec![link.clone()]),
        Value::Tuple(vec![spelling])
    );

    assert_eq!(
        Value::Tuple(vec![number.clone()]),
        Value::Tuple(vec![number.clone()])
    );

    for value in [Value::None, vec![1u8].into(), Value::Tuple(vec![number])] {
        assert!(Cow::<str>::opt_cast_from(&value).is_none());
        assert!(!Cow::<str>::can_cast_from(&&value));
        assert!(!String::can_cast_from(&value));
        assert!(String::opt_cast_from(value.clone()).is_none());
        assert!(Number::opt_cast_from(&value).is_none());
        assert!(!Number::can_cast_from(&&value));
        assert!(<&str>::opt_cast_from(&value).is_none());
        assert!(!<&str>::can_cast_from(&&value));
    }

    assert!(Number::opt_cast_from(&link).is_none());
    assert!(<&str>::opt_cast_from(&link).is_none());
}

#[test]
fn canonical_leaf_conversions_delegate_without_numeric_coercion() {
    use safecast::CastFrom;

    for number in [
        Number::from(false),
        Number::from(true),
        Number::from(0_i64),
        Number::from(-1_i64),
        Number::from(0.5_f64),
        Number::from(-0.5_f64),
        Number::from(-0.0_f64),
        Number::from(f64::NAN),
        Number::from(f64::INFINITY),
        Number::from(number_general::Complex::from([0.0_f64, 0.5])),
        Number::from(number_general::Complex::from([0.0_f64, 0.0])),
    ] {
        let value = Value::Number(number);
        assert!(bool::can_cast_from(&value));
        assert_eq!(
            bool::opt_cast_from(value.clone()),
            Some(bool::cast_from(number))
        );

        assert!(Number::can_cast_from(&value));
        assert_eq!(Number::opt_cast_from(value), Some(number));
    }

    assert_eq!(
        bool::opt_cast_from(Value::Number(0.5_f64.into())),
        Some(true)
    );

    assert_eq!(bool::opt_cast_from(Value::None), Some(false));
    for value in [
        Value::None,
        Value::from("7"),
        Value::Tuple(vec![Value::from(7_u64)]),
        Value::from(vec![7_u8]),
    ] {
        assert!(!Number::can_cast_from(&value));
        assert!(Number::opt_cast_from(value).is_none());
    }
}

#[test]
fn canonical_path_conversions_preserve_literal_and_uri_identity() {
    let native = PathBuf::from(path_label(&["café", "literal%2F"]));
    let link = Link::from(native.clone());
    for value in [Value::String(link.to_string()), Value::Link(link.clone())] {
        assert!(Link::can_cast_from(&value));
        assert_eq!(Link::opt_cast_from(value), Some(link.clone()));
    }

    for value in [Value::String(native.to_string()), Value::Link(link.clone())] {
        assert!(PathBuf::can_cast_from(&value));
        assert_eq!(PathBuf::opt_cast_from(value), Some(native.clone()));
    }

    let escape_looking = Value::String(PathBuf::from(path_label(&["%41"])).to_string());
    assert_eq!(
        PathBuf::opt_cast_from(escape_looking.clone()),
        Some(PathBuf::from(path_label(&["%41"])))
    );

    assert_eq!(
        Link::opt_cast_from(escape_looking).unwrap().path(),
        &PathBuf::from(path_label(&["A"]))
    );

    let malformed_uri = Value::String(PathBuf::from(path_label(&["%GG"])).to_string());
    assert!(!Link::can_cast_from(&malformed_uri));
    assert!(Link::opt_cast_from(malformed_uri.clone()).is_none());
    assert!(PathBuf::opt_cast_from(malformed_uri).is_some());

    let hosted = Value::Link(Link::from((
        pathlink::Host::from_str("https://example.com").unwrap(),
        native,
    )));

    assert!(!PathBuf::can_cast_from(&hosted));
    assert!(PathBuf::opt_cast_from(hosted).is_none());
    for value in [Value::None, Value::from(7_u64), Value::Tuple(vec![])] {
        assert!(!Link::can_cast_from(&value));
        assert!(!PathBuf::can_cast_from(&value));
        assert!(Link::opt_cast_from(value.clone()).is_none());
        assert!(PathBuf::opt_cast_from(value).is_none());
    }

    let root = PathBuf::default();
    assert_eq!(
        PathBuf::opt_cast_from(Value::from(root.to_string())),
        Some(root)
    );
}

#[test]
fn tuple_conversions_delegate_exact_arity_and_move_fields() {
    struct OwnedText(String);

    impl TryCastFrom<Value> for OwnedText {
        fn can_cast_from(value: &Value) -> bool {
            matches!(value, Value::String(_))
        }

        fn opt_cast_from(value: Value) -> Option<Self> {
            match value {
                Value::String(string) => Some(Self(string)),
                _ => None,
            }
        }
    }

    let strings = [
        String::from("one"),
        String::from("two"),
        String::from("three"),
    ];

    let pointers = strings.each_ref().map(|string| string.as_ptr());
    let value = Value::Tuple(strings.into_iter().map(Value::String).collect());
    type Triple = (OwnedText, OwnedText, OwnedText);
    assert!(Triple::can_cast_from(&value));
    let (one, two, three) = Triple::opt_cast_from(value).unwrap();
    assert_eq!([one.0.as_ptr(), two.0.as_ptr(), three.0.as_ptr()], pointers);
    assert_eq!(
        [one.0.as_str(), two.0.as_str(), three.0.as_str()],
        ["one", "two", "three"]
    );

    type Nested = ((Number,), String);
    let nested = Value::Tuple(vec![
        Value::Tuple(vec![Value::from(7_u64)]),
        Value::from("text"),
    ]);

    assert!(Nested::can_cast_from(&nested));
    assert_eq!(
        Nested::opt_cast_from(nested),
        Some(((Number::from(7_u64),), "text".to_owned()))
    );

    for length in 0..=4 {
        let value = Value::Tuple((0..length).map(|_| Value::from("text")).collect());
        assert_eq!(<(String,)>::can_cast_from(&value), length == 1);
        assert_eq!(
            <(String,)>::opt_cast_from(value.clone()).is_some(),
            length == 1
        );

        assert_eq!(<(String, String)>::can_cast_from(&value), length == 2);
        assert_eq!(
            <(String, String)>::opt_cast_from(value.clone()).is_some(),
            length == 2
        );

        assert_eq!(
            <(String, String, String)>::can_cast_from(&value),
            length == 3
        );

        assert_eq!(
            <(String, String, String)>::opt_cast_from(value).is_some(),
            length == 3
        );
    }

    for value in [Value::from("text"), Value::Tuple(vec![Value::None])] {
        assert!(!<(OwnedText,)>::can_cast_from(&value));
        assert!(<(OwnedText,)>::opt_cast_from(value).is_none());
    }

    assert_eq!(
        Vec::<Number>::opt_cast_from(Value::from(7_u64)),
        Some(vec![7_u64.into()])
    );

    assert_eq!(
        Vec::<Number>::opt_cast_from(Value::Tuple(vec![])),
        Some(vec![])
    );

    assert_eq!(
        Vec::<Number>::opt_cast_from(Value::Tuple(vec![7_u64.into()])),
        Some(vec![7_u64.into()])
    );
}

#[test]
fn primitive_constructors_preserve_dtype() {
    macro_rules! integers {
        ($($ty:ty),+) => {$(
            for input in [<$ty>::MIN, 0, <$ty>::MAX] {
                let value = Value::from(input);
                let expected = Number::from(input);
                assert_eq!(value.class(), ValueType::Number(expected.class()));
                assert_eq!(Number::opt_cast_from(value), Some(expected));
            }
        )+};
    }

    integers!(i8, i16, i32, i64, u8, u16, u32, u64);
    for input in [false, true] {
        assert_eq!(
            Value::from(input).class(),
            ValueType::Number(NumberType::Bool)
        );
    }

    macro_rules! floats {
        ($ty:ty, $variant:ident) => {
            for input in [
                0.0,
                -0.0,
                <$ty>::from_bits(1),
                <$ty>::MAX,
                <$ty>::INFINITY,
                <$ty>::NAN,
            ] {
                let Value::Number(Number::Float(number_general::Float::$variant(output))) =
                    Value::from(input)
                else {
                    panic!("float width")
                };
                assert_eq!(output.to_bits(), input.to_bits());
            }
        };
    }

    floats!(f32, F32);
    floats!(f64, F64);
    assert!(matches!(Value::from(vec![1_u8]), Value::Bytes(_)));
}

#[test]
fn canonical_numeric_paths_and_type_order_are_stable() {
    const NUMBER_PATH: PathLabel = path_label(&["state", "scalar", "value", "number"]);
    let root = PathBuf::from(NUMBER_PATH);
    assert_eq!(number_type_path(&NumberType::Number), root);
    let families = [
        (
            "complex",
            vec![
                NumberType::Complex(ComplexType::Complex),
                NumberType::Complex(ComplexType::C32),
                NumberType::Complex(ComplexType::C64),
            ],
            vec!["32", "64"],
        ),
        (
            "float",
            vec![
                NumberType::Float(FloatType::Float),
                NumberType::Float(FloatType::F32),
                NumberType::Float(FloatType::F64),
            ],
            vec!["32", "64"],
        ),
        (
            "int",
            vec![
                NumberType::Int(IntType::Int),
                NumberType::Int(IntType::I8),
                NumberType::Int(IntType::I16),
                NumberType::Int(IntType::I32),
                NumberType::Int(IntType::I64),
            ],
            vec!["8", "16", "32", "64"],
        ),
        (
            "uint",
            vec![
                NumberType::UInt(UIntType::UInt),
                NumberType::UInt(UIntType::U8),
                NumberType::UInt(UIntType::U16),
                NumberType::UInt(UIntType::U32),
                NumberType::UInt(UIntType::U64),
            ],
            vec!["8", "16", "32", "64"],
        ),
    ];

    const VALUE_PATH: PathLabel = path_label(&["state", "scalar", "value"]);
    for (dtype, suffix) in [
        (ValueType::None, "none"),
        (ValueType::Bytes, "bytes"),
        (ValueType::String, "string"),
        (ValueType::Link, "link"),
        (ValueType::Tuple, "tuple"),
    ] {
        let expected = PathBuf::from(VALUE_PATH).append(label(suffix));
        assert_eq!(dtype.path(), expected);
        assert_eq!(ValueType::from_path(&expected), Some(dtype));
    }

    let mut numbers = vec![NumberType::Number, NumberType::Bool];
    assert_eq!(
        number_type_path(&NumberType::Bool),
        root.clone().append(label("bool"))
    );

    for (family, types, widths) in families {
        let prefix = root.clone().append(family.parse::<Id>().unwrap());
        for (i, dtype) in types.into_iter().enumerate() {
            let expected = if i == 0 {
                prefix.clone()
            } else {
                prefix.clone().append(widths[i - 1].parse::<Id>().unwrap())
            };

            assert_eq!(number_type_path(&dtype), expected);
            assert_eq!(ValueType::Number(dtype).path(), expected);
            assert_eq!(number_type_from_path(&expected), Some(dtype));
            numbers.push(dtype);
        }

        assert!(number_type_from_path(&prefix.clone().append(label("128"))).is_none());
        assert!(
            number_type_from_path(&prefix.append(label("32")).append(label("extra"))).is_none()
        );
    }

    assert!(number_type_from_path(&PathBuf::default()).is_none());
    assert!(number_type_from_path(&PathBuf::from(path_label(&["wrong"]))).is_none());
    for left in &numbers {
        for right in &numbers {
            assert_eq!(
                ValueType::Number(*left).cmp(&ValueType::Number(*right)),
                left.cmp(right)
            );
        }
    }

    let kinds = [
        ValueType::None,
        ValueType::Bytes,
        ValueType::Number(NumberType::Bool),
        ValueType::String,
        ValueType::Link,
        ValueType::Tuple,
    ];

    for (i, left) in kinds.iter().enumerate() {
        for (j, right) in kinds.iter().enumerate() {
            assert_eq!(left.cmp(right), i.cmp(&j));
        }
    }

    let values = [
        Value::None,
        Value::from(vec![0_u8]),
        Value::from(0_u64),
        Value::from(""),
        Value::Link(Link::from(PathBuf::default())),
        Value::Tuple(vec![]),
    ];

    for (i, left) in values.iter().enumerate() {
        for (j, right) in values.iter().enumerate() {
            assert_eq!(ValueCollator::default().cmp(left, right), i.cmp(&j));
        }
    }
}

#[test]
fn conversion_predicates_agree_with_consumption() {
    fn owned<T: TryCastFrom<Value>>(value: &Value) {
        let eligible = T::can_cast_from(value);
        assert_eq!(T::opt_cast_from(value.clone()).is_some(), eligible);
        assert_eq!(T::try_cast_from(value.clone(), |_| ()).is_ok(), eligible);
    }

    fn borrowed<'a, T: TryCastFrom<&'a Value>>(value: &'a Value) {
        let eligible = T::can_cast_from(&value);
        assert_eq!(T::opt_cast_from(value).is_some(), eligible);
        assert_eq!(T::try_cast_from(value, |_| ()).is_ok(), eligible);
    }

    let mut inputs = vec![
        Value::None,
        Value::from(vec![1_u8]),
        Value::Link(Link::from(PathBuf::default())),
        Value::from(7_u64),
        Value::from("text"),
        Value::Tuple(vec![]),
        Value::from(""),
        Value::from("%bad"),
        Value::from(ValueType::String.path().to_string()),
        Value::Tuple(vec![Value::from("in"), Value::from(7_u64)]),
        Value::Tuple(vec![Value::from("bad"), Value::None]),
    ];

    for width in 1..=4 {
        inputs.push(Value::Tuple(
            (0..width).map(|_| Value::from(7_u64)).collect(),
        ));
    }

    inputs.push(Value::Tuple(vec![
        Value::Tuple(vec![Value::from(7_u64)]),
        Value::from("text"),
    ]));

    for value in &inputs {
        owned::<Value>(value);
        owned::<Number>(value);
        owned::<bool>(value);
        owned::<String>(value);
        owned::<Link>(value);
        owned::<PathBuf>(value);
        owned::<Id>(value);
        owned::<Arc<[u8]>>(value);
        owned::<bytes::Bytes>(value);
        owned::<ValueType>(value);
        owned::<Bound<Value>>(value);
        owned::<Vec<Number>>(value);
        owned::<(Number,)>(value);
        owned::<(Number, String)>(value);
        owned::<(Number, Number, Number)>(value);
        owned::<((Number,), String)>(value);
        borrowed::<Number>(value);
        borrowed::<&str>(value);
        borrowed::<Cow<'_, str>>(value);
    }
}
