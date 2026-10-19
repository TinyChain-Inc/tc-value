use std::cmp::Ordering;
use std::fmt;
use std::sync::Arc;

use async_hash::{Digest, Hash, Output};
use collate::{Collate, Collator};
use get_size::GetSize;
use number_general::{Number, NumberCollator};
use pathlink::Link;

use crate::ValueType;

/// An owning scalar value with bounded-stack traversal and destruction.
pub struct Value {
    kind: Option<ValueKind>,
}

/// Movable contents of a [`Value`]. Tuple children retain their owning boundary.
#[derive(Clone, Debug, PartialEq)]
pub enum ValueKind {
    None,
    Bytes(Arc<[u8]>),
    Link(Link),
    Number(Number),
    String(String),
    Tuple(Vec<Value>),
}

impl Eq for ValueKind {}

impl Value {
    pub fn kind(&self) -> &ValueKind {
        self.kind.as_ref().expect("attached value")
    }

    pub fn kind_mut(&mut self) -> &mut ValueKind {
        self.kind.as_mut().expect("attached value")
    }

    pub fn into_kind(mut self) -> ValueKind {
        self.kind.take().expect("attached value")
    }

    pub fn class(&self) -> ValueType {
        match self.kind() {
            ValueKind::None => ValueType::None,
            ValueKind::Bytes(_) => ValueType::Bytes,
            ValueKind::Link(_) => ValueType::Link,
            ValueKind::Number(_) => ValueType::Number,
            ValueKind::String(_) => ValueType::String,
            ValueKind::Tuple(_) => ValueType::Tuple,
        }
    }

    pub(crate) fn walk(&self) -> Walk<'_> {
        Walk {
            next: Some(self),
            frames: Vec::new(),
        }
    }
}

impl From<ValueKind> for Value {
    fn from(kind: ValueKind) -> Self {
        Self { kind: Some(kind) }
    }
}

impl Default for Value {
    fn default() -> Self {
        ValueKind::None.into()
    }
}

impl Drop for Value {
    fn drop(&mut self) {
        let Some(ValueKind::Tuple(children)) = self.kind.take() else {
            return;
        };
        let mut frames = vec![children.into_iter()];
        while let Some(frame) = frames.last_mut() {
            match frame.next() {
                Some(value) => {
                    if let ValueKind::Tuple(children) = value.into_kind() {
                        frames.push(children.into_iter());
                    }
                }
                None => {
                    frames.pop();
                }
            }
        }
    }
}

pub(crate) enum Visit<'a> {
    Leaf(&'a ValueKind),
    Tuple { len: usize, capacity: usize },
    End,
}

/// Shared depth-first traversal retains one slice cursor per open tuple.
pub(crate) struct Walk<'a> {
    next: Option<&'a Value>,
    frames: Vec<std::slice::Iter<'a, Value>>,
}

impl<'a> Walk<'a> {
    pub(crate) fn try_next(
        &mut self,
    ) -> Result<Option<Visit<'a>>, std::collections::TryReserveError> {
        self.advance(|frames| frames.try_reserve(1))
    }

    fn advance<E>(
        &mut self,
        reserve: impl FnOnce(&mut Vec<std::slice::Iter<'a, Value>>) -> Result<(), E>,
    ) -> Result<Option<Visit<'a>>, E> {
        let value = match self.next.take() {
            Some(value) => value,
            None => {
                let Some(frame) = self.frames.last_mut() else {
                    return Ok(None);
                };
                match frame.next() {
                    Some(value) => value,
                    None => {
                        self.frames.pop();
                        return Ok(Some(Visit::End));
                    }
                }
            }
        };
        Ok(Some(match value.kind() {
            ValueKind::Tuple(children) => {
                reserve(&mut self.frames)?;
                self.frames.push(children.iter());
                Visit::Tuple {
                    len: children.len(),
                    capacity: children.capacity(),
                }
            }
            leaf => Visit::Leaf(leaf),
        }))
    }
}

impl<'a> Iterator for Walk<'a> {
    type Item = Visit<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.advance(|_| Ok::<_, std::convert::Infallible>(()))
            .unwrap_or_else(|never| match never {})
    }
}

impl Clone for Value {
    fn clone(&self) -> Self {
        let mut frames: Vec<Vec<Value>> = Vec::new();
        for visit in self.walk() {
            let value = match visit {
                Visit::Tuple { len, .. } => {
                    frames.push(Vec::with_capacity(len));
                    continue;
                }
                Visit::Leaf(leaf) => leaf.clone().into(),
                Visit::End => ValueKind::Tuple(frames.pop().expect("open tuple")).into(),
            };
            match frames.last_mut() {
                Some(parent) => parent.push(value),
                None => return value,
            }
        }
        unreachable!("value traversal produces one root")
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        self.walk()
            .zip(other.walk())
            .all(|(left, right)| match (left, right) {
                (Visit::Leaf(left), Visit::Leaf(right)) => left == right,
                (Visit::Tuple { len: left, .. }, Visit::Tuple { len: right, .. }) => left == right,
                (Visit::End, Visit::End) => true,
                _ => false,
            })
    }
}
impl Eq for Value {}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pretty = f.alternate();
        let mut counts = Vec::new();
        for visit in self.walk() {
            if matches!(visit, Visit::End) {
                let count = counts.pop().expect("open tuple");
                if pretty && count != 0 {
                    writeln!(f, ",")?;
                    write!(f, "{:width$}", "", width = counts.len() * 4)?;
                }
                write!(f, "])")?;
                continue;
            }
            if let Some(count) = counts.last_mut() {
                if *count != 0 {
                    write!(f, ",")?;
                    if !pretty {
                        write!(f, " ")?;
                    }
                }
                *count += 1;
                if pretty {
                    writeln!(f)?;
                    write!(f, "{:width$}", "", width = counts.len() * 4)?;
                }
            }
            match visit {
                Visit::Tuple { .. } => {
                    write!(f, "Tuple([")?;
                    counts.push(0);
                }
                Visit::Leaf(leaf) => write!(f, "{leaf:?}")?,
                Visit::End => unreachable!(),
            }
        }
        Ok(())
    }
}

impl<D: Digest> Hash<D> for Value {
    fn hash(self) -> Output<D> {
        Hash::<D>::hash(&self)
    }
}

impl<D: Digest> Hash<D> for &Value {
    fn hash(self) -> Output<D> {
        let mut frames: Vec<(D, bool)> = Vec::new();
        for visit in self.walk() {
            let hash = match visit {
                Visit::Tuple { len, .. } => {
                    frames.push((D::new(), len == 0));
                    continue;
                }
                Visit::Leaf(leaf) => match leaf {
                    ValueKind::None => async_hash::default_hash::<D>(),
                    ValueKind::Bytes(bytes) => D::digest(bytes),
                    ValueKind::Link(link) => Hash::<D>::hash(link),
                    ValueKind::Number(number) => Hash::<D>::hash(*number),
                    ValueKind::String(string) => Hash::<D>::hash(string),
                    ValueKind::Tuple(_) => unreachable!(),
                },
                Visit::End => {
                    let (hasher, empty) = frames.pop().expect("open tuple");
                    if empty {
                        async_hash::default_hash::<D>()
                    } else {
                        hasher.finalize()
                    }
                }
            };
            match frames.last_mut() {
                Some((parent, _)) => parent.update(hash),
                None => return hash,
            }
        }
        unreachable!("value traversal produces one root")
    }
}

impl GetSize for Value {
    fn get_heap_size(&self) -> usize {
        self.walk()
            .map(|visit| match visit {
                Visit::Tuple { capacity, .. } => capacity * std::mem::size_of::<Value>(),
                Visit::Leaf(ValueKind::Bytes(bytes)) => {
                    bytes.len() + 2 * std::mem::size_of::<usize>()
                }
                Visit::Leaf(ValueKind::Link(link)) => link.get_heap_size(),
                Visit::Leaf(ValueKind::String(string)) => string.capacity(),
                _ => 0,
            })
            .sum()
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
        let mut next = Some((left, right));
        let mut frames: Vec<(std::slice::Iter<'_, Value>, std::slice::Iter<'_, Value>)> =
            Vec::new();
        loop {
            let (left, right) = match next.take() {
                Some(pair) => pair,
                None => {
                    let Some((left, right)) = frames.last_mut() else {
                        return Ordering::Equal;
                    };
                    match (left.next(), right.next()) {
                        (Some(left), Some(right)) => (left, right),
                        (None, Some(_)) => return Ordering::Less,
                        (Some(_), None) => return Ordering::Greater,
                        (None, None) => {
                            frames.pop();
                            continue;
                        }
                    }
                }
            };
            let ordering = match (left.kind(), right.kind()) {
                (ValueKind::None, ValueKind::None) => Ordering::Equal,
                (ValueKind::Bytes(left), ValueKind::Bytes(right)) => self.bytes.cmp(left, right),
                (ValueKind::Link(left), ValueKind::Link(right)) => self.link.cmp(left, right),
                (ValueKind::Number(left), ValueKind::Number(right)) => self.number.cmp(left, right),
                (ValueKind::String(left), ValueKind::String(right)) => self.string.cmp(left, right),
                (ValueKind::Tuple(left), ValueKind::Tuple(right)) => {
                    frames.push((left.iter(), right.iter()));
                    continue;
                }
                _ => left.class().cmp(&right.class()),
            };
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
    }
}
