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

## Orchestration and Local Task Memory

Use Beads (`bd`) as the local task ledger for multi-step work. Beads data is
machine-local project memory and must stay out of git:

- keep `.beads/` ignored and never stage, commit, push, or PR Beads data
- if `bd` is missing, install or initialize it locally before splitting work
- create one coordinator bead for the GitHub issue or project goal, and attach
  child beads for implementation, docs, verification, and review tracks
- use the GitHub issue number as external context in bead titles or comments,
  but keep GitHub issues and local Beads state separate
- before starting work, record the intended split in Beads; while working, have
  each subagent comment progress, findings, blockers, and verification commands
  on its assigned bead
- the orchestrator checks Beads for status, reconciles progress, and tells
  subagents to close their beads when their assigned work is done
- close child beads only after their work is implemented or explicitly ruled out,
  and close the coordinator only after the PR is merged or the user-visible goal
  is otherwise complete
- close subagent sessions after their result is integrated or no longer needed

When using subagents, treat the main agent as orchestrator: delegate focused,
bounded tasks; require written findings in Beads; review their outputs before
editing; and keep final integration, commits, pushes, and PRs under the main
agent's control.

## GitHub Issue Workflow

Use the `gh` CLI as part of normal task management:

- inspect open issues before starting work with `gh issue list` and `gh issue view`
- do not push directly to `develop`; use a feature branch and PR for every repo
  change
- keep PRs scoped to one bead or one GitHub issue when practical
- merge only after local checks and required GitHub checks pass
- after merge, sync local `develop`, close the bead, and continue with the next
  open issue
- use the Blipcoard GitHub Project as the planning board:
  `https://github.com/users/acozy03/projects/2`
- inspect project fields before creating or moving work with
  `gh project view 2 --owner acozy03` and
  `gh project field-list 2 --owner acozy03`
- add new phase, feature, and follow-up issues to the Blipcoard project
- assign issue ownership when creating or moving tasks; default owner is
  `@acozy03` unless the user says otherwise
- reference the relevant issue number in branch names, commits, PRs, and status
  updates when one exists
- update project status after completing work or moving a task forward
- create focused sub-issues or follow-up issues when a task reveals new scoped
  work that should be tracked separately
- apply the repo labels and project fields/tags that match the work type, phase,
  and current status instead of leaving new tasks uncategorized
- set the project Iteration field for new tasks because the Blipcoard board is
  filtered by iteration
- set task Priority by engineering judgment based on user impact, dependency
  order, risk, and urgency; do not default every task to the same priority
- keep phase and deliverable issues aligned with `docs/mvp-phases.md`
