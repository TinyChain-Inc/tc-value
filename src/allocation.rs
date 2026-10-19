//! Retained allocation bounds for codec inspection, without decoding values.

use std::mem::size_of;

use destream::de::{Error, Inspection};
use hr_id::Id;

use crate::Value;

impl Value {
    /// The heap contribution of one codec inspection event.
    ///
    /// Sum these contributions while inspecting one value, then add the enclosing
    /// owner's inline size. Bounds cover the canonical decoder's retained
    /// allocations; they neither interpret type tags nor construct values.
    pub fn allocation_bound<E: Error>(event: &Inspection<'_>) -> Result<usize, E> {
        let overflow = || E::custom("value allocation bound overflow");
        // A string may become a Link. Each segment owns an Arc, and spilled
        // paths can retain twice their occupied number of Id slots.
        let segment = 2 * size_of::<usize>() + 2 * size_of::<Id>();
        match event {
            Inspection::TextChunk(bytes) => bytes
                .iter()
                .filter(|&&byte| byte == b'/')
                .count()
                .checked_mul(segment)
                .ok_or_else(overflow),
            Inspection::TextEnd { capacity_bound } => {
                capacity_bound.checked_add(segment).ok_or_else(overflow)
            }
            Inspection::Sequence { len, size_hint } => {
                let len = (*len).max(size_hint.unwrap_or(0));
                let capacity = if len == 0 {
                    0
                } else {
                    len.checked_next_power_of_two().ok_or_else(overflow)?.max(4)
                };
                capacity
                    .checked_mul(size_of::<Value>())
                    .ok_or_else(overflow)
            }
            Inspection::TypedArray { len, element_size } => {
                let bytes = len.checked_mul(*element_size).ok_or_else(overflow)?;
                let capacity = if bytes == 0 {
                    0
                } else {
                    bytes
                        .checked_next_power_of_two()
                        .ok_or_else(overflow)?
                        .max(8)
                };
                capacity
                    .checked_add(2 * size_of::<usize>())
                    .ok_or_else(overflow)
            }
            Inspection::Map { .. } => Ok(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use destream::de;
    use get_size::GetSize;

    struct Heap(usize);
    impl de::FromStream for Heap {
        type Context = ();
        async fn from_stream<D: de::Decoder>(_: (), decoder: &mut D) -> Result<Self, D::Error> {
            let mut heap = 0usize;
            decoder
                .inspect_any(|event| {
                    heap = heap
                        .checked_add(Value::allocation_bound::<D::Error>(&event)?)
                        .ok_or_else(|| de::Error::custom("allocation overflow"))?;
                    Ok(())
                })
                .await?;
            Ok(Self(heap))
        }
    }

    #[tokio::test]
    async fn inspection_covers_values_without_byte_multipliers() {
        for count in [0, 1, 2, 4, 5, 17] {
            let values = [
                Value::from(vec![Value::from("content"); count]),
                Value::from(vec![7_u8; count]),
                Value::from(
                    format!(
                        "https://example.com/{}",
                        (0..count).map(|i| format!("part{i}/")).collect::<String>()
                    )
                    .parse::<pathlink::Link>()
                    .unwrap(),
                ),
                Value::from("quoted \" / text".repeat(count * 256)),
            ];
            for value in values {
                let Heap(bound) =
                    destream_json::try_decode((), destream_json::encode(&value).unwrap())
                        .await
                        .unwrap();
                let decoded: Value =
                    destream_json::try_decode((), destream_json::encode(&value).unwrap())
                        .await
                        .unwrap();
                assert!(
                    bound >= decoded.get_heap_size(),
                    "{count}: {bound} < {}",
                    decoded.get_heap_size()
                );
            }
        }
        let value = Value::from("x".repeat(4096));
        let Heap(bound) = destream_json::try_decode((), destream_json::encode(&value).unwrap())
            .await
            .unwrap();
        assert!(
            bound < 8192,
            "plain text must be charged by bytes, not Value size"
        );
    }

    #[test]
    fn inspection_capacity_overflow_is_an_error() {
        for event in [
            Inspection::TextEnd {
                capacity_bound: usize::MAX,
            },
            Inspection::Sequence {
                len: usize::MAX,
                size_hint: None,
            },
            Inspection::TypedArray {
                len: usize::MAX,
                element_size: 8,
            },
        ] {
            assert!(Value::allocation_bound::<destream_json::de::Error>(&event).is_err());
        }
    }
}
