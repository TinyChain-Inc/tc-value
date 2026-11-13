# tc-value

Borrowed `ToStream` encoding delegates through native values without cloning
strings or tuple trees. Byte values retain shared ownership of their payload;
owned and borrowed encoding produce the same representation.

`tc-value` owns TinyChain's canonical scalar value vocabulary. It is independent
of host state, transactions, collections, and transport adapters.

Native public handlers live in `tc-state`. They delegate arithmetic to
`number-general`'s checked `try_*` methods and convert failures into request
errors. Integral handler arguments use checked casts, rejecting fractions,
nonfinite values, and discarded imaginary components. Value metadata, collation,
and logical predicates remain infallible.

Value owns borrowed number/string inspection, native equality semantics, and text
conversion. `Number::opt_cast_from(&value)` and `<&str>::opt_cast_from(&value)`
recognize their variants without coercion. Public `eq` uses Value's `PartialEq`:
Links and strings are distinct, including inside tuples. Compare a parsed Link
explicitly when textual URI equivalence is intended. Number equality continues
to delegate to `number-general` across numerical representations.
`String::opt_cast_from(value)` moves strings and formats numbers or links; bytes,
tuples, and None have no text conversion.
`Cow::<str>::opt_cast_from(&value)` provides the same conversion while borrowing
existing strings. State delegates these conversions while owning request parsing,
method selection, container traversal, and execution.

## Conversions

`TryCastFrom` owns fallible conversions; container conversions delegate to their
fields and move consumed values. Numeric extraction accepts an embedded Number,
without parsing strings or narrowing it. Boolean conversion delegates numerical
truthiness to `number-general`; `None` converts to false. Use checked numeric
casts when a request requires a particular numeric type.

Link conversion accepts a Link or parses URI text. Native PathBuf conversion
accepts literal path text or a hostless Link; it never discards a host. Thus
escape-looking identifier text stays literal in native paths and is decoded only
when parsing Links. Parsing at boundaries that need error causes should retain
the parser's `Result` rather than erase it through an optional cast.

Tuple conversions require exactly one, two, or three fields. `Vec<T>` converts
tuple elements individually and retains its scalar-to-singleton convention.
Neither conversion clones payloads merely to extract fields.

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
so they remain distinct from tuples. Explicit numeric tags request checked
conversion to the declared type: range failures, discarded fractions or imaginary
components, nonzero-to-zero underflow, and Boolean values other than zero or one
are rejected. Finite precision rounding and representable subnormals remain valid.
The wire representation is unchanged; explicit lossy conversions remain available
through `number-general`'s native casting APIs. Ordinary literals do not need tags.
Text codecs may infer a different integer width or signedness without changing
the numerical value.

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

Primitive Boolean, signed/unsigned 8–64-bit integer, and f32/f64 constructors
preserve the source Number dtype. `Vec<u8>` still constructs bytes. Converting
Bytes to Value copies directly into the retained Arc allocation, without an
intermediate vector; it does not provide zero-copy ownership transfer.

Numeric type paths use one private family/width mapping for parsing and formatting.
ValueType declaration order preserves canonical type collation; NumberType retains
its own ordering. Value itself has no intrinsic ordering.
