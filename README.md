# tc-value

Core value representations shared across TinyChain crates. This crate will house
the canonical `Value` enum (numbers, strings, tuples, etc.) used by the IR,
state subsystem, and adapters.

## Current status

- [x] Canonical `Value` enum implemented with variants:
	- `None`
	- `Bool`
	- `Number`
	- `String`
	- `Link`
	- `Map`
	- `Tuple`
- [x] `destream`/JSON round-trip support for all variants.
- [x] Bool-as-number semantics: JSON booleans decode as `Value::Number(Number::Bool(...))`.
- [x] Unit tests for literal and nested map/tuple round-trips.

## Encoding notes

- `None`, `Bool`, `Number`, and `String` encode as plain JSON literals.
- `Map` encodes as a plain JSON object of nested `Value`s.
- `Tuple` encodes as a plain JSON array of nested `Value`s.
- `Link` encodes as a single-entry map keyed by the link path (v1-compatible form).

Typed value envelopes (`/state/scalar/value/...`) remain accepted where required for
compatibility, but canonical emission prefers the plain JSON forms above.
