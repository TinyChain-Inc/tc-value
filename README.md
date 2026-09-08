# tc-value

`tc-value` owns TinyChain's canonical scalar value vocabulary. It is independent
of host state, transactions, collections, and transport adapters.

## Values

`Value` has six variants:

- `None`
- `Bytes(Arc<[u8]>)`
- `Link`
- `Number` (including booleans)
- `String`
- `Tuple`

Maps belong to the IR/state collection structure rather than `Value`. Booleans
use `Number::Bool`; there is no separate `Value::Bool` variant.

Each variant owns its canonical type URI, size accounting, collation,
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
