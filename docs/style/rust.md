# Rust

## Tooling

- **rustfmt** runs with its defaults, which follow the Rust Style Guide.
- **Clippy** runs with `-D warnings`.
  - The lint policy lives in `[workspace.lints]`, and every crate opts in with `lints.workspace = true`.
  - Deny `unwrap_used`, `panic`, `todo`, `unimplemented`, `dbg_macro`, `as_conversions`, `undocumented_unsafe_blocks` and `allow_attributes_without_reason`.
  - Warn on `unreachable_pub`, `missing_debug_implementations` and `unused_qualifications`.
  - `clippy.toml` allows unwrap, expect, panics and indexing in tests.
  - Leave pedantic, nursery and restriction off as groups. If one of their lints earns its place, enable it on its own.
- **Exceptions:** a deliberate exception is an `#[expect(lint, reason = "…")]` on the item it covers. It fails once the exception is no longer needed, which `#[allow]` never does.
- **rustdoc** runs with `-D warnings`.
- **cargo-deny** checks licences, advisories, duplicate versions and sources.

## Tests

The layout is the one in the Rust Book's chapter 11.3, Test Organization:

- **Unit tests** go in a `#[cfg(test)] mod tests` at the foot of the file they test, with `use super::*`.
- **Integration tests** go in `<crate>/tests/<area>.rs`, one file per area, and exercise the crate through its public interface.
  - Shared helpers go in `tests/common/mod.rs`. A `tests/common.rs` would be compiled and run as a test crate of its own.
  - `src/` holds the code and its inline unit tests.
- **Reaching past the production interface:** when integration tests need more than production exposes (an in-memory adapter, a clock to move, a way in that skips I/O), give the crate a small public surface for them under `cfg(any(test, debug_assertions))`, and start each test file that uses it with `#![cfg(debug_assertions)]`.
- **Fixtures:** describe the situation with one value, such as a scenario a test starts from, rather than with a builder of many setters.
- **Bug fixes** start with a test that fails.
- **Names** state the behaviour in the domain's words: `fn a_snoozed_rule_comes_back_when_its_timer_ends`.

## Panics

Production code is total, and it has to be wherever a panic aborts the process (`panic = "abort"`, or code behind an FFI):

- **Validate input where it enters**, and carry the checked type on, so nothing downstream can fail on it.
- **`expect`** only an invariant the code itself guarantees, under `#[expect(clippy::expect_used, reason = "…")]`.
- **Convert numbers** with `From` and `TryFrom`.
- **Lock a `Mutex` once per entry point** and pass `&mut` down. Locking it again on the same thread deadlocks or panics.

## Errors

- **Libraries** use thiserror enums, one per kind of failure, each carrying its cause. Binaries use anyhow at the top.
- **An error is a type that implements `Error`.** A `String` is a message, not an error.
- **Errors reach the caller who can act on them,** as a refusal, a check or an exit code. A fallback value hides the failure.
- **Where an error's text can carry private data,** the log gets the error's kind.

## Types and interfaces

These follow the Rust API Guidelines:

- **Identifiers are newtypes** with their validation and their equality built in (C-NEWTYPE, C-VALIDATE). They serialize as plain strings with `#[serde(try_from = "String", into = "String")]`.
- **Conversions are trait implementations:** `From`, `TryFrom` and `FromStr` (C-CONV). Getters have no `get_` prefix (C-GETTER).
- **Derives:** derive `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash` and `Default` wherever the fields allow (C-COMMON-TRAITS, C-DEBUG).
- **Visibility:** items inside a crate are `pub(crate)`, and `pub` marks its real interface.
- **Seams:** a seam is worth its trait when two adapters fill it, typically production and in-memory.
- **Edition 2024 idioms:** let-chains, `is_some_and` and `take_if`.

## Dependencies

- **Declare each dependency once** in `[workspace.dependencies]`. Crates inherit it with `.workspace = true`.
- **Solved problems use well-established crates:** serde, toml and serde_json for formats, jiff for time, idna for domains, tempfile for atomic writes, clap for arguments, and thiserror and anyhow for errors.
- **Test-only crates are dev-dependencies.** A crate enables the features it uses itself, rather than getting them through feature unification.

## Documentation

- **`//!`** says what a module is for. **`///`** says what an item is or does, in the domain's plain words, and names the decision record it carries out, if there is one.
- **`//` comments** state a constraint, an invariant or a non-obvious reason. They describe the code as it is.
