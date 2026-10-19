# tc-value

Borrowed `ToStream` encoding delegates through native values without cloning
strings or tuple trees. Byte values retain shared ownership of their payload;
owned and borrowed encoding produce the same representation.

`tc-value` owns TinyChain's canonical scalar value vocabulary. It is independent
of host state, transactions, collections, and transport adapters.

## Values

`Value` owns a movable `ValueKind` with six variants:

- `None`
- `Bytes(Arc<[u8]>)`
- `Link`
- `Number` (including booleans)
- `String`
- `Tuple`

Maps belong to the IR/state collection structure rather than `Value`. Booleans
use `Number::Bool`. Inspect a value with `kind()` or `kind_mut()`, consume it with
`into_kind()`, and construct it with `Value::from(kind)` or its typed conversions.
The owner keeps destruction, cloning, equality, collation, hashing, formatting,
and retained size accounting iterative. Tuples retain ordinary `Vec<Value>`
children; no alternate tree representation is needed.

Each variant has its canonical type URI, size accounting, collation,
conversions, semantic traversal, and symmetric `destream` codec. Bytes are an
ordinary bounded value and are used at real byte boundaries such as raw WASM
installation; they are not a package or server-specific payload.

Plain literals use their natural wire form where one exists. Typed values use
the canonical `/state/scalar/value/...` representation required for an
unambiguous round trip. Readers accept the form emitted by writers; this crate
does not maintain compatibility envelopes.

```bash
cargo test --all-targets --all-features
```

`Value::allocation_bound` reports the retained heap contribution of a codec
inspection event. Persistence adapters sum these checked bounds before decoding;
framing and borrowed token traversal remain owned by the codec. The bound covers
canonical decoder capacities and link allocations without interpreting wire tags
or constructing a Value.

Canonical decoding retains one construction frame per open tuple and uses the
codec's shallow container cursor. Encoding consumes the same value traversal as
shallow events. Both paths preserve wire representations and source ownership;
errors and cancellation drop unfinished values through their owning boundaries.

Decoding and encoding reserve growing construction and traversal containers
fallibly. Infallible Rust traits, including `Clone` and `Drop`, retain ordinary
allocation behavior while traversing on a bounded call stack.
