# tc-value

Borrowed `ToStream` encoding delegates through native values without cloning
strings or tuple trees. Byte values retain shared ownership of their payload;
owned and borrowed encoding produce the same representation.

`tc-value` owns TinyChain's canonical scalar value vocabulary. It is independent
of host state, transactions, collections, and transport adapters.

## Values

`Value` is a plain enum with six variants:

- `None`
- `Bytes(Arc<[u8]>)`
- `Link`
- `Number` (including booleans)
- `String`
- `Tuple`

Maps belong to the IR/state collection structure rather than `Value`. Booleans
use `Number::Bool`. Match variants directly and construct them directly or through
typed conversions. Tuples own ordinary `Vec<Value>` children.

Codecs and Scalar/State-to-Value conversions accept at most 64 nested Value tuples.
Conversions include embedded Values when checking the resulting depth, before
constructing a tuple tree. Trusted native constructors must respect this contract:
cloning, comparison, hashing, formatting, size accounting, and destruction use
ordinary recursive enum operations. Scalar and State retain their own independent
ownership and execution mechanisms.

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

`GetSize` measures actual retained storage, including container capacity and link
allocations. Persistence adapters decode values before cache admission; this crate
retains structural decoding limits rather than predicting allocation.

Canonical decoding retains one construction frame per open tuple and rejects a
65th tuple before constructing it. Streaming encoding uses codec-local traversal
and preserves wire representations. Errors and cancellation release unfinished
values through ordinary Rust ownership. Construction and encoding cursor vectors
reserve fallibly; infallible Rust traits retain ordinary allocation behavior.
