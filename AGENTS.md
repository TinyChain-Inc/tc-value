# tc-value Agent Notes

`tc-value` defines the canonical scalar envelope shared by every TinyChain crate.
Treat it as the single source of truth for how numbers, strings, tuples, and
other primitives are identified, serialized, and round-tripped through the IR.

## Modeling rules

- Keep `Value` minimal and transport-agnostic. New variants must map cleanly to
  `tc-ir::Scalar` and be representable without host context or bespoke codecs.
- Extend `ValueType` in lockstep with `Value`; every variant needs a stable path
  rooted at `/state/scalar/value/...`. Never bypass the URI builder or embed
  literal strings—use the helpers in `class.rs`.
- Reuse shared primitives (`Number` from `number-general`, `TCRef`, common tuple
  types). Do not introduce crate-specific wrapper structs unless they are
  reusable by `tc-state`, `tc-collection`, and adapters.
- Avoid feature-flag forks. The same `Value` definitions must compile on every
  target (kernel, PyO3, WASM) so the control plane stays deterministic.

## Serialization and wire format

- `de::FromStream`/`en::IntoStream` are the canonical encoding. Keep them in sync
  with the JSON contract used by HTTP/WebSocket adapters: a map whose single key
  is the value type path. Do not add alternate envelopes or ad-hoc verbs.
- Normalize scalar coercions through `number_general::Number`. If you add string,
  tuple, or binary support, route all parsing through shared helper modules so
  clients cannot mint incompatible representations.
- When introducing a new scalar, add round-trip tests that decode both the
  canonical typed envelope and the plain JSON literal (where applicable) so
  adapters remain forgiving without diverging from the TinyChain schema.

## Testing and coordination

- Run `cargo test -p tc-value` after changing `Value`, `ValueType`, or any codec
  logic. Add targeted unit tests instead of broad integration suites—this crate
  underpins most others, so fast, focused coverage keeps development tight.
- Update dependent docs (`README.md`, `ROADMAP.md`, crate-specific guides) if the
  scalar surface changes. `tc-ir`, `tc-state`, and `tc-server` rely on this crate
  staying in lockstep; flag any breaking changes in their respective `AGENTS.md`
  before merging.
