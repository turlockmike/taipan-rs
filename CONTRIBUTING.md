# Contributing

Thanks for your interest in improving this project. It's small, dependency-free,
and test-driven — easy to reason about and easy to extend.

## Principles

- **Zero runtime dependencies.** The project builds with only the Rust standard
  library — no crates. This is deliberate (it builds with bare `rustc`, and the
  PRNG/JSON are auditable in-tree). PRs that add a dependency need a strong
  justification.
- **Test-driven.** Every feature ships with tests. Logic lives in the library
  crate so it's unit-testable without spawning a process. Prefer adding a
  failing test first, then making it pass.
- **Determinism is a guarantee, not a nicety.** A given `--seed` plus a given
  sequence of actions must always produce the same game. Anything that consumes
  randomness must draw from the seeded `Rng`; never use wall-clock time or
  ambient entropy. There are tests asserting this — keep them green.
- **Errors over silent no-ops.** Invalid input should produce a clear error, not
  a quietly ignored action.

## Getting started

```sh
git clone https://github.com/turlockmike/taipan-rs
cd taipan-rs
cargo test
cargo run -- play
```

Read the **Architecture** section of the [README](README.md) first — the module
table tells you where things live. The dependency layering is intentional;
respect it (e.g. `market.rs` does not know about ports — the `economy` layer
translates a port into a price bias).

## Before opening a PR

Run these locally; CI runs the same:

```sh
cargo test                  # all tests must pass
cargo clippy --all-targets  # no warnings
cargo fmt                   # formatted
```

## Good first contributions

- **A new economy model.** Add a variant to `EconomyMode` and its bias logic in
  `economy.rs`. Nothing else should need to change. Add a winnability test like
  the existing ones.
- **More arrival events** in `events.rs` (weather, opportunities, Li Yuen's
  counter-offers). Keep them seed-deterministic and add coverage.
- **Deeper fidelity to the 1982 original** — e.g. the periodic economy
  re-inflation (every 12 months the source bumps base prices), or Li Yuen's
  protection/lieutenant mechanics. The original BASIC is a good reference.

## Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/): `feat:`,
`fix:`, `docs:`, `refactor:`, `test:`, `chore:`. Keep the subject imperative and
under ~72 characters.

## License

By contributing, you agree that your contributions are licensed under the
project's [MIT License](LICENSE).
