# tc-value Roadmap

This crate will house the canonical TinyChain `Value` enum and supporting
serialization/helpers. Track work here so IR/state/adapters stay in sync.

## Phase 1 – Stub + numeric support

1. **Create `Value` enum.** Start with a minimal enum containing at least
   `Number(UInt)` using the `number-general` crate for arbitrary precision.
2. **Serde/destream impls.** Implement `Serialize`, `Deserialize`, and
   `destream::ToStream/FromStream` so `Value` can cross HTTP/PyO3 boundaries.
3. **Unit tests.** Add tests covering numeric round-trips and basic formatting.

## Phase 2 – Extended variants

1. Add string, boolean, tuple/map variants mirroring the existing `tc-transact`
   semantics.
2. Provide conversions to/from the legacy TinyChain JSON envelopes so existing
   installers/tests keep working.
3. Coordinate with `tc-ir` and `tc-state` to migrate their value types to this
   crate once the API stabilizes.

## Phase 3 – Integration

1. Update `tc-ir`, `tc-server`, `client/py`, and `client/js` to depend on
   `tc-value` instead of bespoke value definitions.
2. Add CI tests ensuring a serialized `Value` round-trips across Rust ↔ PyO3 ↔
   JS clients.
