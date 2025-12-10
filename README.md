# tc-value

Core value representations shared across TinyChain crates. This crate will house
the canonical `Value` enum (numbers, strings, tuples, etc.) used by the IR,
state subsystem, and adapters.

## Current status

- [ ] Stub `Value` enum with initial variants (e.g., `Number`).
- [ ] Implement numeric support using `number-general`.
- [ ] Add serde/destream implementations so `Value` can flow across adapters.
- [ ] Wire unit tests and cross-crate integration tests once the initial API stabilizes.
