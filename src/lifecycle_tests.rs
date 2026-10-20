use std::cmp::Ordering;
use std::fmt::{self, Write};
use std::sync::{
    atomic::{AtomicBool, Ordering as AtomicOrdering},
    Arc,
};
use std::task::Poll;

use async_hash::{Digest, Hash, Sha256};
use bytes::Bytes;
use collate::Collate;
use destream::{en, IntoStream};
use futures::{FutureExt, StreamExt, TryStreamExt};
use get_size::GetSize;
use number_general::Number;

use crate::class::NativeClass;
use crate::{Value, ValueCollator, ValueType};

fn nested(depth: usize, mut leaf: Value) -> Value {
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
fn lifecycle_preserves_leaf_and_tuple_semantics() {
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

struct Discard;
impl Write for Discard {
    fn write_str(&mut self, _: &str) -> fmt::Result {
        Ok(())
    }
}

#[test]
fn bounded_lifecycle_uses_normal_stack() {
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
    write!(Discard, "{value:?}").unwrap();
    write!(
        Discard,
        "{:#?}",
        nested(crate::MAX_VALUE_DEPTH, Value::default())
    )
    .unwrap();
    drop(value);
    drop(clone);
}

#[derive(Clone, Copy, Debug)]
enum Codec {
    Json,
    Tbon,
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
                en::Event::Value(Value::from(ValueType::Tuple.path().to_string())),
                en::Event::SeqStart(Some(1)),
            ]
        });
        let extra_open = self.1.then_some(en::Event::SeqStart(Some(1)));
        let extra_close = self.1.then_some(en::Event::End);
        let events = open
            .chain(extra_open)
            .chain([en::Event::Value(Value::default())])
            .chain(extra_close)
            .chain((0..2 * self.0).map(|_| en::Event::End))
            .map(Ok::<_, E::Error>);
        encoder.encode_events(futures::stream::iter(events))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn canonical_depth_boundary_and_typed_tuples_across_codecs() {
    tokio::spawn(async {
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

struct Dropped(Arc<AtomicBool>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.store(true, AtomicOrdering::SeqCst);
    }
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
        let dropped = Arc::new(AtomicBool::new(false));
        let guard = Dropped(dropped.clone());
        let mut prefix = Some(Bytes::from(prefix));
        let source = futures::stream::poll_fn(move |_| {
            let _ = &guard;
            match prefix.take() {
                Some(prefix) => Poll::Ready(Some(Ok::<_, std::io::Error>(prefix))),
                None => Poll::Pending,
            }
        });
        assert!(destream_json::try_decode::<_, _, Value>((), source)
            .now_or_never()
            .is_none());
        assert!(dropped.load(AtomicOrdering::SeqCst));
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
async fn typed_payload_validation_and_extra_entries_preserve_wire_behavior() {
    for input in [
        r#"{"/state/scalar/value/tuple":null}"#,
        r#"{"/state/scalar/value/unsupported":null}"#,
        r#"{"/state/scalar/value/link":"not a link"}"#,
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
    let input = br#"{"/state/scalar/value/tuple":[null],"ignored":{"deep":[1,2]}}"#;
    assert_eq!(
        Codec::Json.decode(input.to_vec(), 1).await.unwrap(),
        vec![Value::default()].into()
    );
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
