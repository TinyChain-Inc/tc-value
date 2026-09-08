# tc-value Agent Notes

`tc-value` defines the canonical scalar values shared by every TinyChain crate.
Treat it as the single source of truth for `None`, bytes, links, numbers,
strings, and tuples.

## Modeling rules

- Keep `Value` minimal and transport-agnostic. New variants must map cleanly to
  `tc-ir::Scalar` and be representable without host context or bespoke codecs.
- Extend `ValueType` in lockstep with `Value`; every variant needs a stable path
  rooted at `/state/scalar/value/...`. Never bypass the URI builder or embed
  literal strings—use the helpers in `class.rs`.
- Match paths via `PathLabel` slices instead of ad-hoc string comparisons. Add
  new labels/segments beside the types they describe and reuse `path_matches`
  helpers so every caller enforces TinyChain `Id` validation consistently.
- Reuse shared primitives such as `Number` from `number-general`. Do not
  introduce crate-specific wrapper structs unless they are
  reusable by `tc-state`, `tc-collection`, and adapters.
- `Value` has intrinsic equality but no intrinsic ordering. All ordered storage,
  ranges, and stream merges must use `ValueCollator`, which delegates numbers to
  `NumberCollator` and recursively collates tuples. Do not implement `Ord` or
  `PartialOrd` for `Value`, compare serialized forms, or use `Collator<Value>`.
- Avoid feature-flag forks. The same `Value` definitions must compile on every
  target (kernel, PyO3, WASM) so the control plane stays deterministic.

## Serialization and wire format

- `de::FromStream`/`en::IntoStream` are the canonical encoding. Keep them
  symmetric and do not add alternate envelopes.
- Normalize scalar coercions through `number_general::Number`. If you add string,
  tuple, or binary support, route all parsing through shared helper modules so
  clients cannot mint incompatible representations.
- When introducing a new scalar, add canonical round-trip, malformed-input,
  collation, size, and type-path tests as applicable.

## Testing and coordination

- Run `cargo test -p tc-value` after changing `Value`, `ValueType`, or any codec
  logic. Add targeted unit tests instead of broad integration suites—this crate
  underpins most others, so fast, focused coverage keeps development tight.
- Update dependent docs (`README.md`, `ROADMAP.md`, crate-specific guides) if the
  scalar surface changes. `tc-ir`, `tc-state`, and `tc-server` rely on this crate
  staying in lockstep; flag any breaking changes in their respective `AGENTS.md`
  before merging.
- `destream` is the canonical codec here. A concrete transport may enable an
  adapter feature, but transport compatibility logic does not belong in Value.
