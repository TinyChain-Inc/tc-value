# Contributing to `tc-value`

Thank you for helping improve the TinyChain value layer. This crate will soon
live in its own public repository, so we follow the same lightweight rules as
the rest of the TinyChain Open-Source Project.

## Ground rules

1. **Represent your work.** By opening a pull request or submitting patches you
   affirm that the contribution is your original work (or that you have the
   right to contribute it) and that you grant all necessary rights to the
   TinyChain Open-Source Project.
2. **Transfer of rights.** All contributions are made under the repository
   license (Apache 2.0). By contributing you assign copyright in that work to
   the TinyChain Open-Source Project so that the community can redistribute and
   relicense the code as needed.
3. **Minimal surface area.** Prefer general primitives over bespoke helpers and
   document any deviations from the shared design guidance in
   [`AGENTS.md`](../AGENTS.md) at the workspace root.
4. **Style + tooling.** Run `cargo fmt` and `cargo clippy --all-targets
   --all-features` before sending patches. Follow the shared
   [`CODE_STYLE.md`](./CODE_STYLE.md) and keep imports ordered/grouped per that
   guide.
5. **Testing.** Only add tests that raise confidence in critical paths. If a
   change affects serialization or user-visible behavior, include at least one
   focused unit test.

## Contribution process

1. Fork/branch, make your changes, and keep commits focused.
2. Format + lint (`cargo fmt`, `cargo clippy --all-targets --all-features`).
3. Run any targeted tests (`cargo test` or the specific integration test your
   change touches).
4. Open a pull request describing *why* the change is needed and how it respects
   the TinyChain design philosophy.

Questions about scope or architecture? Open an issue or start a discussion
before writing code so we can keep the minimal, general TinyChain surface area
intact.
