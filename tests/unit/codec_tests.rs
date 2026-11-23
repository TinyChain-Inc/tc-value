use std::cmp::Ordering;
use std::sync::Arc;
use std::task::Poll;

use async_hash::{Hash, Sha256};
use bytes::Bytes;
use collate::Collate;
use destream::{en, IntoStream};
use futures::{FutureExt, StreamExt, TryStreamExt};
use get_size::GetSize;
use number_general::{Complex, ComplexType, FloatType, IntType, Number, NumberType, UIntType};
use safecast::TryCastFrom;

use super::tests::nested;
use crate::class::NativeClass;
use crate::{Value, ValueCollator, ValueType};

#[derive(Clone, Copy, Debug)]
enum Codec {
    Json,
    Tbon,
}

#[tokio::test]
async fn natural_links_escape_only_ambiguous_value_strings() {
    for codec in [Codec::Json, Codec::Tbon] {
        for text in [
            "https://example.com/resource",
            "HTTP://example.com/",
            "/",
            "/state/scalar/value/tuple",
            "/state/scalar/ref/cond",
            "/literal%25name/%CE%BB",
        ] {
            let link = Value::from(text.parse::<pathlink::Link>().unwrap());
            let wire = codec.encode(text.to_owned()).await;
            assert_eq!(codec.decode(wire, 1).await.unwrap(), link);
            for value in [link, Value::from(text.to_owned())] {
                let bytes = codec.encode(&value).await;
                assert_eq!(bytes, codec.encode(value.clone()).await);
                assert_eq!(codec.decode(bytes, 1).await.unwrap(), value);
                let tuple = Value::Tuple(vec![value.clone(), Value::Tuple(vec![value])]);
                assert_eq!(
                    codec.decode(codec.encode(&tuple).await, 1).await.unwrap(),
                    tuple
                );
            }
        }

        for text in [
            "ordinary",
            "42",
            "$id",
            "ftp://example.com/x",
            "http://",
            "/bad%escape",
        ] {
            let value = Value::from(text);
            assert_eq!(
                codec.encode(&value).await,
                codec.encode(text.to_owned()).await
            );

            assert_eq!(
                codec.decode(codec.encode(&value).await, 1).await.unwrap(),
                value
            );
        }

        for payload in [0_u64, 42] {
            let map = std::collections::BTreeMap::from([("https://example.com/x", payload)]);
            assert!(codec.decode(codec.encode(map).await, 1).await.is_err());
        }

        let map = std::collections::BTreeMap::from([("https://example.com/x", Vec::<u64>::new())]);
        assert!(codec.decode(codec.encode(map).await, 1).await.is_err());
    }

    let string = Value::from("https://example.com/resource");
    assert_eq!(
        Codec::Json.encode(&string).await,
        br#"{"/state/scalar/value/string":"https://example.com/resource"}"#
    );
}

#[tokio::test]
async fn decoding_compacts_owned_containers_and_preserves_literal_links() {
    for codec in [Codec::Json, Codec::Tbon] {
        for count in [0, 1, 3, 5, 9, 17] {
            let value = Value::Tuple(vec![Value::from("text"); count]);
            let decoded = codec.decode(codec.encode(&value).await, 1).await.unwrap();
            assert_eq!(decoded, value);
            assert!(decoded.get_size() <= value.get_size());
        }

        for path in [
            "/state/scalar/value/tuple",
            "/state/scalar/op/post",
            "/state/scalar/ref/cond",
            "/a/b/c/d/e/f/g/h/i/j",
        ] {
            let value = Value::Link(path.parse().unwrap());
            let borrowed = codec.encode(&value).await;
            assert_eq!(borrowed, codec.encode(value.clone()).await);
            let decoded = codec.decode(borrowed, 1).await.unwrap();
            assert_eq!(decoded, value);
            assert!(decoded.get_size() <= value.get_size());
        }
    }
}

impl Codec {
    async fn encode<'a>(self, value: impl IntoStream<'a> + 'a) -> Vec<u8> {
        match self {
            Self::Json => destream_json::encode(value)
                .unwrap()
                .try_fold(Vec::new(), |mut out, bytes| async move {
                    out.extend(bytes);
                    Ok(out)
                })
                .await
                .unwrap(),
            Self::Tbon => tbon::en::encode(value)
                .unwrap()
                .try_fold(Vec::new(), |mut out, bytes| async move {
                    out.extend(bytes);
                    Ok(out)
                })
                .await
                .unwrap(),
        }
    }

    async fn decode(self, bytes: Vec<u8>, chunk: usize) -> Result<Value, String> {
        let chunks = bytes
            .chunks(chunk)
            .map(Bytes::copy_from_slice)
            .map(Ok::<_, std::io::Error>);
        match self {
            Self::Json => destream_json::try_decode((), futures::stream::iter(chunks))
                .await
                .map_err(|e| e.to_string()),
            Self::Tbon => tbon::de::try_decode((), futures::stream::iter(chunks))
                .await
                .map_err(|e| e.to_string()),
        }
    }
}

struct TypedTuples(usize, bool);

impl<'a> IntoStream<'a> for TypedTuples {
    fn into_stream<E: en::Encoder<'a>>(self, encoder: E) -> Result<E::Ok, E::Error> {
        let open = (0..self.0).flat_map(|_| {
            [
                en::Event::MapStart(Some(1)),
                en::Event::Value(Some(ValueType::Tuple.path())),
                en::Event::SeqStart(Some(1)),
            ]
        });

        let extra_open = self.1.then_some(en::Event::SeqStart(Some(1)));
        let extra_close = self.1.then_some(en::Event::End);
        let events = open
            .chain(extra_open)
            .chain([en::Event::Value(None::<pathlink::PathBuf>)])
            .chain(extra_close)
            .chain((0..2 * self.0).map(|_| en::Event::End))
            .map(Ok::<_, E::Error>);

        encoder.encode_events(futures::stream::iter(events))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn canonical_depth_boundary_and_typed_tuples_across_codecs() {
    tokio::spawn(async {
        let value = nested(crate::MAX_VALUE_DEPTH, "leaf".into());
        let clone = value.clone();
        assert_eq!(value, clone);
        assert_eq!(
            ValueCollator::default().cmp(&value, &clone),
            Ordering::Equal
        );
        assert_eq!(Hash::<Sha256>::hash(&value), Hash::<Sha256>::hash(&clone));
        assert_eq!(
            value.get_heap_size(),
            crate::MAX_VALUE_DEPTH * std::mem::size_of::<Value>() + 4
        );
        assert!(!format!("{value:?}").is_empty());
        assert!(!format!("{:#?}", nested(crate::MAX_VALUE_DEPTH, Value::default())).is_empty());
        drop(value);
        drop(clone);

        for codec in [Codec::Json, Codec::Tbon] {
            for chunk in [1, 4096] {
                for depth in [64, 65] {
                    let value = nested(depth, Value::default());
                    let encoded = codec.encode(&value).await;
                    assert_eq!(encoded, codec.encode(value.clone()).await);
                    let decoded = codec.decode(encoded, chunk).await;
                    if depth == 64 {
                        assert_eq!(decoded.unwrap(), value, "{codec:?}");
                    } else {
                        assert!(decoded.is_err(), "{codec:?}: depth 65 accepted");
                    }
                    let typed = codec.encode(TypedTuples(64, depth == 65)).await;
                    let decoded = codec.decode(typed, chunk).await;
                    if depth == 64 {
                        assert_eq!(decoded.unwrap(), nested(64, Value::default()));
                    } else {
                        assert!(decoded.is_err(), "{codec:?}: typed depth 65 accepted");
                    }
                }
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interrupted_construction_and_owned_encoding_release_deep_values() {
    tokio::spawn(async {
        let complete = Codec::Json
            .encode(nested(63, Value::from("completed")))
            .await;
        let mut prefix = vec![b'['];
        prefix.extend(complete);
        prefix.push(b',');
        let malformed = [prefix.as_slice(), b"]"].concat();
        assert!(Codec::Json.decode(malformed, 4096).await.is_err());
        let owner = Arc::new(());
        let released = Arc::downgrade(&owner);
        let mut prefix = Some(Bytes::from(prefix));
        let source = futures::stream::poll_fn(move |_| {
            let _ = &owner;
            match prefix.take() {
                Some(prefix) => Poll::Ready(Some(Ok::<_, std::io::Error>(prefix))),
                None => Poll::Pending,
            }
        });
        assert!(released.upgrade().is_some());
        assert!(destream_json::try_decode::<_, _, Value>((), source)
            .now_or_never()
            .is_none());
        assert!(released.upgrade().is_none());
        for polled in [false, true] {
            let mut json =
                destream_json::encode(nested(crate::MAX_VALUE_DEPTH, Value::default())).unwrap();
            let mut tbon =
                tbon::en::encode(nested(crate::MAX_VALUE_DEPTH, Value::default())).unwrap();
            if polled {
                assert!(json.next().await.is_some());
                assert!(tbon.next().await.is_some());
            }
            drop(json);
            drop(tbon);
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn typed_payload_validation_preserves_wire_behavior() {
    for codec in [Codec::Json, Codec::Tbon] {
        for suffix in [
            "number",
            "number/uint",
            "number/uint/64",
            "number/uint/8",
            "number/int",
            "number/int/64",
            "number/float",
        ] {
            let path: pathlink::PathBuf = format!("/state/scalar/value/{suffix}").parse().unwrap();
            let class = ValueType::from_path(&path).unwrap();
            let input = std::collections::BTreeMap::from([(path.to_string(), 7_u64)]);
            let bytes = codec.encode(input).await;
            let value = codec.decode(bytes, 1).await.unwrap();
            assert_eq!(value, Value::from(7_u64), "{codec:?}: {suffix}");
            assert!(class.accepts(&value.class()), "{codec:?}: {suffix}");
        }

        for (dtype, number, expected) in [
            (
                NumberType::Int(IntType::I8),
                Number::from(-128i64),
                Number::from(-128i8),
            ),
            (
                NumberType::Int(IntType::I8),
                Number::from(127f64),
                Number::from(127i8),
            ),
            (
                NumberType::UInt(UIntType::U8),
                Number::from(255u64),
                Number::from(255u8),
            ),
            (NumberType::Bool, Number::from(0u64), Number::from(false)),
            (NumberType::Bool, Number::from(1u64), Number::from(true)),
            (
                NumberType::Float(FloatType::F32),
                Number::from(1.1f64),
                Number::from(1.1f32),
            ),
            (
                NumberType::Float(FloatType::F32),
                Number::from(-0.0f64),
                Number::from(-0.0f32),
            ),
            (
                NumberType::Float(FloatType::F32),
                Number::from(f64::from(f32::MIN_POSITIVE * f32::EPSILON)),
                Number::from(f32::MIN_POSITIVE * f32::EPSILON),
            ),
            (
                NumberType::Complex(ComplexType::C32),
                Number::from(Complex::from([1.1f64, 2.2])),
                Number::from(Complex::from([1.1f32, 2.2])),
            ),
            (
                NumberType::Complex(ComplexType::C32),
                Number::from(Complex::from([1f32, f32::MIN_POSITIVE * f32::EPSILON])),
                Number::from(Complex::from([1f32, f32::MIN_POSITIVE * f32::EPSILON])),
            ),
        ] {
            let class = ValueType::Number(dtype);
            let input = std::collections::BTreeMap::from([(class.path().to_string(), number)]);
            let bytes = codec.encode(input).await;
            let decoded = codec.decode(bytes, 1).await.unwrap();
            assert_eq!(decoded, Value::Number(expected), "{codec:?}: {dtype:?}");
            assert!(class.accepts(&decoded.class()));
            // JSON may encode floating -0 as integer zero; TBON retains its dtype.
            if let (Codec::Tbon, Value::Number(Number::Float(value)), Number::Float(expected)) =
                (codec, decoded, expected)
            {
                assert_eq!(
                    f64::from(value).is_sign_negative(),
                    f64::from(expected).is_sign_negative()
                );
            }
        }

        for (dtype, number) in [
            (NumberType::Int(IntType::I8), Number::from(128i64)),
            (NumberType::Int(IntType::I8), Number::from(-129i64)),
            (NumberType::Int(IntType::I8), Number::from(1.5f64)),
            (NumberType::Int(IntType::I64), Number::from(u64::MAX)),
            (NumberType::UInt(UIntType::U8), Number::from(-1i64)),
            (NumberType::Bool, Number::from(2u64)),
            (NumberType::Float(FloatType::F32), Number::from(f64::MAX)),
            (
                NumberType::Float(FloatType::F32),
                Number::from(f64::MIN_POSITIVE),
            ),
            (
                NumberType::Int(IntType::I64),
                Number::from(Complex::from([1f64, 1.])),
            ),
            (
                NumberType::Complex(ComplexType::C32),
                Number::from(Complex::from([1f64, f64::MIN_POSITIVE])),
            ),
            (
                NumberType::Complex(ComplexType::C32),
                Number::from(Complex::from([f64::MAX, 1f64])),
            ),
        ] {
            let class = ValueType::Number(dtype);
            let input = std::collections::BTreeMap::from([(class.path().to_string(), number)]);
            let bytes = codec.encode(input).await;
            assert!(
                codec.decode(bytes, 1).await.is_err(),
                "{codec:?}: {dtype:?}"
            );
        }

        for number in [
            Number::from(number_general::Complex::from([3_f32, 4_f32])),
            Number::from(number_general::Complex::from([3_f64, 4_f64])),
        ] {
            let value = Value::Number(number);
            let bytes = codec.encode(&value).await;
            assert_eq!(bytes, codec.encode(value.clone()).await);
            let decoded = codec.decode(bytes, 1).await.unwrap();
            assert_eq!(decoded, value);
            assert_eq!(decoded.class(), value.class());
        }
    }

    let float_path = ValueType::Number(NumberType::Float(FloatType::F32)).path();
    let input = format!("{{\"{float_path}\":-0.0}}");
    let Value::Number(Number::Float(value)) =
        Codec::Json.decode(input.into_bytes(), 1).await.unwrap()
    else {
        panic!("typed float")
    };
    assert!(f64::from(value).is_sign_negative());

    for input in [
        r#"{"/state/scalar/value/tuple":null}"#,
        r#"{"/state/scalar/value/unsupported":null}"#,
        r#"{"/state/scalar/value/link":"not a link"}"#,
        r#"{"/state/scalar/value/number/uint/64":"not a number"}"#,
        r#"{"/state/scalar/value/number/complex/32":[3]}"#,
        r#"{"/state/scalar/value/number/complex/32":[3,4,5]}"#,
        r#"{"/state/scalar/value/number/complex/32":[[1,0],2]}"#,
        r#"{}"#,
    ] {
        assert!(
            Codec::Json
                .decode(input.as_bytes().to_vec(), 1)
                .await
                .is_err(),
            "{input}"
        );
    }
}

#[tokio::test]
async fn typed_records_require_unit_null_and_exactly_one_entry() {
    for codec in [Codec::Json, Codec::Tbon] {
        for payload in [
            Value::from(7_u64),
            Value::from("invalid"),
            Value::Tuple(Vec::new()),
        ] {
            let entries =
                en::MapStream::from(futures::stream::iter([(ValueType::None.path(), payload)]));

            assert!(codec.decode(codec.encode(entries).await, 1).await.is_err());
        }

        for (class, payload) in [
            (ValueType::None, Value::None),
            (ValueType::String, Value::from("literal")),
            (ValueType::Tuple, Value::Tuple(vec![Value::None])),
        ] {
            let key = class.path().to_string();
            let entries =
                en::MapStream::from(futures::stream::iter([(key.clone(), payload.clone())]));

            assert_eq!(
                codec.decode(codec.encode(entries).await, 1).await.unwrap(),
                payload
            );

            for extra_key in [key.clone(), "ignored".to_owned()] {
                let entries = en::MapStream::from(futures::stream::iter([
                    (key.clone(), payload.clone()),
                    (extra_key, Value::None),
                ]));

                assert!(codec.decode(codec.encode(entries).await, 1).await.is_err());
            }
        }
    }
}

#[tokio::test]
async fn extra_type_entry_rejects_without_waiting_for_its_payload() {
    for codec in [Codec::Json, Codec::Tbon] {
        for (class, payload) in [
            (ValueType::String, Value::from("literal")),
            (ValueType::Tuple, Value::Tuple(Vec::new())),
        ] {
            let entries = en::MapStream::from(futures::stream::iter([(class.path(), payload)]));
            let mut prefix = codec.encode(entries).await;
            assert_eq!(prefix.pop(), Some(b'}'));
            prefix.extend_from_slice(match codec {
                Codec::Json => b",\"ignored\":[",
                Codec::Tbon => b"\"ignored\"[",
            });

            let owner = Arc::new(());
            let released = Arc::downgrade(&owner);
            let source = futures::stream::iter([Bytes::from(prefix)])
                .chain(futures::stream::pending())
                .inspect(move |_| {
                    let _ = &owner;
                });

            let decode = async {
                match codec {
                    Codec::Json => destream_json::decode::<_, Value>((), source)
                        .await
                        .map_err(|e| e.to_string()),
                    Codec::Tbon => tbon::de::decode::<_, Value>((), source)
                        .await
                        .map_err(|e| e.to_string()),
                }
            };

            assert!(decode
                .now_or_never()
                .expect("reject without reading extra payload")
                .is_err());

            assert!(released.upgrade().is_none());
        }
    }
}

#[tokio::test]
async fn typed_link_validates_its_separator_before_parsing_the_link() {
    use destream::de;
    struct PriorMapOrder;
    impl de::FromStream for PriorMapOrder {
        type Context = ();
        async fn from_stream<D: de::Decoder>(_: (), decoder: &mut D) -> Result<Self, D::Error> {
            struct Visitor;
            impl de::Visitor for Visitor {
                type Value = PriorMapOrder;
                fn expecting() -> &'static str {
                    "a typed link"
                }
                async fn visit_map<A: de::MapAccess>(
                    self,
                    mut map: A,
                ) -> Result<Self::Value, A::Error> {
                    map.next_key::<String>(()).await?;
                    let raw = map.next_value::<String>(()).await?;
                    raw.parse::<pathlink::Link>().map_err(de::Error::custom)?;
                    Ok(PriorMapOrder)
                }
            }
            decoder.decode_map(Visitor).await
        }
    }
    for suffix in [
        r#""not a link" !}"#,
        r#""/valid" !}"#,
        r#""not a link""#,
        r#""not a link"}"#,
    ] {
        let input = format!("{{\"/state/scalar/value/link\":{suffix}");
        let stream = || {
            futures::stream::iter([Ok::<_, std::io::Error>(Bytes::copy_from_slice(
                input.as_bytes(),
            ))])
        };
        let previous = destream_json::try_decode::<_, _, PriorMapOrder>((), stream())
            .await
            .err()
            .unwrap()
            .to_string();
        let current = destream_json::try_decode::<_, _, Value>((), stream())
            .await
            .err()
            .unwrap()
            .to_string();
        assert_eq!(current, previous, "{input}");
    }
}

#[tokio::test]
async fn link_uri_keys_remain_distinct_from_literal_paths() {
    for name in ["λ", "literal%name", "%2f"] {
        let path = pathlink::PathBuf::from(name.parse::<pathlink::Id>().unwrap());
        let link = pathlink::Link::from(path.clone());
        let value = Value::from(link.clone());
        assert_ne!(path.to_string(), link.to_string());
        let uri = match name {
            "λ" => "/%CE%BB",
            "literal%name" => "/literal%25name",
            "%2f" => "/%252f",
            _ => unreachable!(),
        };

        assert_eq!(link.to_string(), uri);
        let expected_json = format!("\"{uri}\"");
        assert_eq!(Codec::Json.encode(&value).await, expected_json.as_bytes());
        let expected_tbon = format!("\"{uri}\"");
        assert_eq!(Codec::Tbon.encode(&value).await, expected_tbon.as_bytes());
        for codec in [Codec::Json, Codec::Tbon] {
            let expected_link = format!("\"{uri}\"");
            let expected_path = format!("\"{path}\"");
            assert_eq!(codec.encode(&link).await, expected_link.as_bytes());
            assert_eq!(codec.encode(link.clone()).await, expected_link.as_bytes());
            assert_eq!(codec.encode(&path).await, expected_path.as_bytes());
            assert_eq!(codec.encode(path.clone()).await, expected_path.as_bytes());
            let borrowed = codec.encode(&value).await;
            assert_eq!(borrowed, codec.encode(value.clone()).await);
            assert_eq!(codec.decode(borrowed, 1).await.unwrap(), value);
        }
    }
}

#[tokio::test]
async fn byte_conversion_preserves_owned_and_borrowed_wire_data() {
    let unique = Bytes::from(vec![1_u8, 2]);
    assert!(unique.is_unique());
    let unique = Value::from(unique);
    assert_eq!(
        Arc::<[u8]>::opt_cast_from(unique).unwrap().as_ref(),
        &[1, 2]
    );

    let original = Bytes::from(vec![0_u8, 1, 127, 255]);
    let alias = original.clone();
    for input in [
        Bytes::new(),
        original,
        alias.slice(1..3),
        Bytes::from_static(b"static"),
    ] {
        let expected = input.to_vec();
        let retained = input.clone();
        let value = Value::from(input);
        assert_eq!(retained.as_ref(), expected.as_slice());
        drop(retained);
        let Value::Bytes(payload) = &value else {
            panic!("bytes")
        };

        assert_eq!(payload.as_ref(), expected.as_slice());
        for codec in [Codec::Json, Codec::Tbon] {
            let borrowed = codec.encode(&value).await;
            assert_eq!(borrowed, codec.encode(value.clone()).await);
            assert_eq!(codec.decode(borrowed, 1).await.unwrap(), value);
        }
    }

    assert_eq!(alias.as_ref(), &[0, 1, 127, 255]);
}
