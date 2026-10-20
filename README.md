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

`ValueType::Number(NumberType)` retains the numerical type reported by
`Value::class()`. Type declarations use `accepts` for membership: generic Number
accepts every number, a generic numerical family accepts its members, and a
concrete type requires an exact match. Schema validation never coerces values.

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
does not maintain compatibility envelopes. As in v1, booleans, integers, and
floats encode as bare literals. Complex numbers carry their numerical type path
so they remain distinct from tuples. Explicit numeric tags request conversion to
the declared type; ordinary literals do not need tags. Text codecs may infer a
different integer width or signedness without changing the numerical value.

```bash
cargo test --all-targets --all-features
```

Private unit-test implementations live in `tests/unit/`, included by conditional
module declarations in `src`. Module identities and private access are preserved;
these files do not introduce separate integration-test targets.

`GetSize` measures actual retained storage, including container capacity and link
allocations. Persistence adapters decode values before cache admission; this crate
retains structural decoding limits rather than predicting allocation.

Canonical decoding retains one construction frame per open tuple and rejects a
65th tuple before constructing it. Primitive encoding delegates directly to the
native variant; only tuples use codec-local iterative traversal. Both preserve
wire representations. Errors and cancellation release unfinished values through
ordinary Rust ownership. Construction and encoding cursor vectors
reserve fallibly; infallible Rust traits retain ordinary allocation behavior.
