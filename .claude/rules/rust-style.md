# Rust style: declarative, combinator-driven

- Prefer iterator chains and `Option`/`Result` combinators (`map`,
  `and_then`, `filter`, `ok_or`, `collect::<Result<_, _>>()`) over
  imperative loops, mutable accumulators, and early-return plumbing.
- Decode with slice patterns and exhaustive matches so message layouts
  read as tables mirroring the spec document.
- Encode invariants in types — newtypes with validating constructors,
  enums instead of magic numbers. Make invalid states unrepresentable
  rather than checking at runtime.
- Panics are compile errors in library code (workspace lints in the root
  `Cargo.toml` enforce this); tests are exempt via `clippy.toml`.
- "Prefer" is not "always": a `fold` building a byte, or a match arm
  returning a `vec![...]` literal, is already declarative. Don't force
  point-free chains where they hurt readability.
- Code is self-documenting: no comments that narrate what the code does.
  A comment must state something the code cannot — spec-table provenance
  for magic bytes, a non-obvious trick, an invariant. Doc comments (`///`)
  on public items are API documentation, not narration; keep them.

`z1-protocol/src/sysex.rs` is the calibration reference for this style.
