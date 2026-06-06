# blipcoard Agents

This project vendors the full Rust skills library as a submodule at:

- `.rust-skills/`

For Rust development in this repository, use the Rust skills guidance from:

- `.rust-skills/AGENTS.md`

## Required Rust Guidance

For all Rust work in `blipcoard`, follow the guidance from `.rust-skills/AGENTS.md`
as the default project instruction set.

Use the Rust skill library in `.rust-skills/skills/` going forward, with:

- `rust-router` as the primary routing skill for Rust questions
- `coding-guidelines` for code style and naming
- `unsafe-checker` for unsafe or FFI-related work
- `rust-learner` for Rust version/crate guidance

Use the relevant meta-skills based on the problem:

- ownership and borrowing: `m01-ownership`
- resource management / smart pointers: `m02-resource`
- mutability patterns: `m03-mutability`
- zero-cost abstractions: `m04-zero-cost`
- type-driven design: `m05-type-driven`
- error handling: `m06-error-handling`
- concurrency / async: `m07-concurrency`
- domain modeling: `m09-domain`
- performance work: `m10-performance`
- ecosystem and crate choices: `m11-ecosystem`
- lifecycle / project evolution: `m12-lifecycle`
- domain error modeling: `m13-domain-error`
- mental models / teaching framing: `m14-mental-model`
- anti-pattern review: `m15-anti-pattern`

Use domain-specific skills where applicable:

- CLI work: `domain-cli`
- web/UI-adjacent backend work: `domain-web`
- ML/AI-adjacent Rust work: `domain-ml`
- cloud-native / services work: `domain-cloud-native`

## Project-Specific Note

`blipcoard` is a Rust workspace with a runtime-first architecture:

- `blipd` daemon
- `blip` CLI
- desktop app as an additional surface

When making Rust changes here:

- prefer workspace-aware changes over one-off crate-local shortcuts
- keep platform-specific behavior isolated from shared core logic
- avoid `unwrap()` in library code unless the invariant is explicit and local
- add tests for storage, routing, and CLI behavior when behavior changes
