use std::str::FromStr;
use std::sync::Arc;

use destream::de::FromStream;
use destream::en::EncodeMap;
use destream::{de, en, IntoStream};
use number_general::Number;
use pathlink::{Link, PathBuf};

use crate::class::NativeClass;
use crate::{Value, ValueType, VALUE_PREFIX};

fn typed_value<E: de::Error>(key: &str) -> Result<Option<ValueType>, E> {
    let Ok(path) = key.parse::<PathBuf>() else {
        return Ok(None);
    };
    if let Some(class) = ValueType::from_path(&path) {
        return Ok(Some(class));
    }
    let prefix_len = VALUE_PREFIX[..].len();
    if path.len() == prefix_len + 1 && path[..prefix_len] == VALUE_PREFIX[..] {
        return Err(E::custom(format!("unsupported value type path {path}")));
    }
    Ok(None)
}

/// Decode the payload of a recognized Value type path at the current decoder slot.
/// Recognized payloads complete their current map child before returning. Unknown
/// keys leave both input and cursor untouched. The caller owns any remaining map
/// entries and their validation policy.
pub async fn decode_typed_value<D: de::Decoder>(
    key: &str,
    decoder: &mut D,
    map: &mut de::Container,
    remaining: usize,
) -> Result<Option<Value>, D::Error> {
    let Some(class) = typed_value::<D::Error>(key)? else {
        return Ok(None);
    };
    let value = if class == ValueType::Tuple {
        if decoder.peek_kind().await? != de::Kind::Seq {
            return Err(de::Error::custom("expected a tuple payload"));
        }
        let value = decode_value(decoder, remaining).await?;
        decoder.finish_child(map).await?;
        value
    } else {
        decode_leaf(class, decoder, map).await?
    };
    Ok(Some(value))
}

async fn decode_leaf<D: de::Decoder>(
    class: ValueType,
    decoder: &mut D,
    map: &mut de::Container,
) -> Result<Value, D::Error> {
    Ok(match class {
        ValueType::Bytes => {
            let bytes = bytes::Bytes::from_stream((), decoder).await?;
            decoder.finish_child(map).await?;
            bytes.into()
        }
        ValueType::Number => {
            let number = Number::from_stream((), decoder).await?;
            decoder.finish_child(map).await?;
            number.into()
        }
        ValueType::None => {
            de::IgnoredAny::from_stream((), decoder).await?;
            decoder.finish_child(map).await?;
            Value::default()
        }
        ValueType::String => {
            let string = String::from_stream((), decoder).await?;
            decoder.finish_child(map).await?;
            string.into()
        }
        ValueType::Link => {
            let link = String::from_stream((), decoder).await?;
            decoder.finish_child(map).await?;
            Link::from_str(&link)
                .map_err(|e| de::Error::custom(e.to_string()))?
                .into()
        }
        ValueType::Tuple => unreachable!("tuple payloads have a construction frame"),
    })
}

async fn drain_map<D: de::Decoder>(
    decoder: &mut D,
    mut map: de::Container,
) -> Result<(), D::Error> {
    // Preserve the canonical typed-value map's existing tolerance of extra entries.
    while map.slot() != de::Slot::End {
        de::IgnoredAny::from_stream((), decoder).await?;
        decoder.finish_child(&mut map).await?;
    }
    Ok(())
}

struct Frame {
    sequence: de::Container,
    items: Vec<Value>,
    wrapper: Option<de::Container>,
}

impl de::FromStream for Value {
    type Context = ();

    async fn from_stream<D: de::Decoder>(_: (), decoder: &mut D) -> Result<Self, D::Error> {
        decode_value(decoder, crate::MAX_VALUE_DEPTH).await
    }
}

/// Decode a Value within an enclosing owner's remaining structural depth.
pub async fn decode_value<D: de::Decoder>(
    decoder: &mut D,
    remaining: usize,
) -> Result<Value, D::Error> {
    let mut frames: Vec<Frame> = Vec::new();
    loop {
        let mut wrapper = None;
        let kind = decoder.peek_kind().await?;
        let mut value = match kind {
            de::Kind::Leaf => Some(decoder.decode_any(LeafVisitor).await?),
            de::Kind::Seq => None,
            de::Kind::Map => {
                let mut map = decoder.open_container(de::Kind::Map, None).await?;
                if map.slot() == de::Slot::End {
                    return Err(de::Error::custom("expected TinyChain type path key"));
                }
                let key = String::from_stream((), decoder).await?;
                decoder.finish_child(&mut map).await?;
                match typed_value::<D::Error>(&key)? {
                    Some(ValueType::Tuple) => {
                        wrapper = Some(map);
                        None
                    }
                    Some(class) => {
                        let value = decode_leaf(class, decoder, &mut map).await?;
                        drain_map(decoder, map).await?;
                        Some(value)
                    }
                    None => {
                        let link = Link::from_str(&key)
                            .map_err(|_| de::Error::custom("map values are not supported"))?;
                        de::IgnoredAny::from_stream((), decoder).await?;
                        decoder.finish_child(&mut map).await?;
                        drain_map(decoder, map).await?;
                        Some(link.into())
                    }
                }
            }
        };
        if value.is_none() {
            if frames.len() >= remaining.min(crate::MAX_VALUE_DEPTH) {
                return Err(de::Error::custom("Value tuple nesting exceeds 64"));
            }
            let sequence = decoder.open_container(de::Kind::Seq, None).await?;
            let empty = sequence.slot() == de::Slot::End;
            frames.try_reserve(1).map_err(de::Error::custom)?;
            frames.push(Frame {
                sequence,
                items: Vec::new(),
                wrapper,
            });
            if !empty {
                continue;
            }
        }
        loop {
            if let Some(child) = value.take() {
                let Some(parent) = frames.last_mut() else {
                    return Ok(child);
                };
                decoder.finish_child(&mut parent.sequence).await?;
                parent.items.try_reserve(1).map_err(de::Error::custom)?;
                parent.items.push(child);
                if parent.sequence.slot() != de::Slot::End {
                    break;
                }
            }
            let frame = frames.pop().expect("completed sequence");
            if let Some(mut map) = frame.wrapper {
                decoder.finish_child(&mut map).await?;
                drain_map(decoder, map).await?;
            }
            value = Some(frame.items.into());
        }
    }
}

struct LeafVisitor;
impl de::Visitor for LeafVisitor {
    type Value = Value;
    fn expecting() -> &'static str {
        "a TinyChain scalar value"
    }

    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::default())
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::default())
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(value.into())
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(Number::from(value).into())
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        Ok(value.into())
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        Ok(Number::from(value).into())
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
        Ok(value.into())
    }
}

struct LeafRef<'a>(&'a Value);
impl<'en> IntoStream<'en> for LeafRef<'en> {
    fn into_stream<E: en::Encoder<'en>>(self, encoder: E) -> Result<E::Ok, E::Error> {
        match self.0 {
            Value::None => encoder.encode_unit(),
            Value::Bytes(bytes) => {
                let mut map = encoder.encode_map(Some(1))?;
                map.encode_entry(
                    ValueType::Bytes.path().to_string(),
                    bytes::Bytes::from_owner(Arc::clone(bytes)),
                )?;
                map.end()
            }
            Value::Link(link) => {
                let mut map = encoder.encode_map(Some(1))?;
                map.encode_entry(link.to_string(), Vec::<()>::new())?;
                map.end()
            }
            Value::Number(number) => number.into_stream(encoder),
            Value::String(string) => string.into_stream(encoder),
            Value::Tuple(_) => unreachable!("tuple encoding uses events"),
        }
    }
}

struct Leaf(Value);
impl<'en> IntoStream<'en> for Leaf {
    fn into_stream<E: en::Encoder<'en>>(self, encoder: E) -> Result<E::Ok, E::Error> {
        match self.0 {
            Value::None => encoder.encode_unit(),
            Value::Bytes(bytes) => {
                let mut map = encoder.encode_map(Some(1))?;
                map.encode_entry(
                    ValueType::Bytes.path().to_string(),
                    bytes::Bytes::from_owner(bytes),
                )?;
                map.end()
            }
            Value::Link(link) => {
                let mut map = encoder.encode_map(Some(1))?;
                map.encode_entry(link.to_string(), Vec::<()>::new())?;
                map.end()
            }
            Value::Number(number) => number.into_stream(encoder),
            Value::String(string) => string.into_stream(encoder),
            Value::Tuple(_) => unreachable!("tuple encoding uses events"),
        }
    }
}

impl<'en> en::ToStream<'en> for Value {
    fn to_stream<E: en::Encoder<'en>>(&'en self, encoder: E) -> Result<E::Ok, E::Error> {
        let mut next = Some(self);
        let mut frames: Vec<std::slice::Iter<'_, Value>> = Vec::new();
        let events = std::iter::from_fn(move || {
            let value = match next.take() {
                Some(value) => value,
                None => match frames.last_mut()?.next() {
                    Some(value) => value,
                    None => {
                        frames.pop();
                        return Some(Ok(en::Event::End));
                    }
                },
            };
            Some(Ok(match value {
                Value::Tuple(children) => {
                    if let Err(error) = frames.try_reserve(1) {
                        return Some(Err(en::Error::custom(error)));
                    }
                    frames.push(children.iter());
                    en::Event::SeqStart(Some(children.len()))
                }
                leaf => en::Event::Value(LeafRef(leaf)),
            }))
        });
        encoder.encode_events(futures::stream::iter(events))
    }
}

impl<'en> IntoStream<'en> for Value {
    fn into_stream<E: en::Encoder<'en>>(self, encoder: E) -> Result<E::Ok, E::Error> {
        let mut next = Some(self);
        let mut frames: Vec<std::vec::IntoIter<Value>> = Vec::new();
        let events = std::iter::from_fn(move || {
            let value = match next.take() {
                Some(value) => value,
                None => match frames.last_mut()?.next() {
                    Some(value) => value,
                    None => {
                        frames.pop();
                        return Some(Ok(en::Event::End));
                    }
                },
            };
            Some(Ok(match value {
                Value::Tuple(children) => {
                    let len = children.len();
                    if let Err(error) = frames.try_reserve(1) {
                        return Some(Err(en::Error::custom(error)));
                    }
                    frames.push(children.into_iter());
                    en::Event::SeqStart(Some(len))
                }
                leaf => en::Event::Value(Leaf(leaf)),
            }))
        });
        encoder.encode_events(futures::stream::iter(events))
    }
}
