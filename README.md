# Rewriting Project Claw Code

<p align="center">
  <strong>⭐ The fastest repo in history to surpass 50K stars, reaching the milestone in just 2 hours after publication ⭐</strong>
</p>

<p align="center">
  <a href="https://star-history.com/#instructkr/claw-code&Date">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=instructkr/claw-code&type=Date&theme=dark" />
      <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=instructkr/claw-code&type=Date" />
      <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=instructkr/claw-code&type=Date" width="600" />
    </picture>
  </a>
</p>

<p align="center">
  <img src="assets/clawd-hero.jpeg" alt="Claw" width="300" />
</p>

<p align="center">
  <strong>Autonomously maintained by lobsters/claws — not by human hands</strong>
</p>

<p align="center">
  <a href="https://github.com/Yeachan-Heo/clawhip">clawhip</a> ·
  <a href="https://github.com/code-yeongyu/oh-my-openagent">oh-my-openagent</a> ·
  <a href="https://github.com/Yeachan-Heo/oh-my-claudecode">oh-my-claudecode</a> ·
  <a href="https://github.com/Yeachan-Heo/oh-my-codex">oh-my-codex</a> ·
  <a href="https://discord.gg/6ztZB9jvWq">UltraWorkers Discord</a>
</p>

> [!IMPORTANT]
> The active Rust workspace now lives in [`rust/`](./rust). Start with [`USAGE.md`](./USAGE.md) for build, auth, CLI, session, and parity-harness workflows, then use [`rust/README.md`](./rust/README.md) for crate-level details.

> Want the bigger idea behind this repo? Read [`PHILOSOPHY.md`](./PHILOSOPHY.md) and Sigrid Jin's public explanation: https://x.com/realsigridjin/status/2039472968624185713

> Shout-out to the UltraWorkers ecosystem powering this repo: [clawhip](https://github.com/Yeachan-Heo/clawhip), [oh-my-openagent](https://github.com/code-yeongyu/oh-my-openagent), [oh-my-claudecode](https://github.com/Yeachan-Heo/oh-my-claudecode), [oh-my-codex](https://github.com/Yeachan-Heo/oh-my-codex), and the [UltraWorkers Discord](https://discord.gg/6ztZB9jvWq).

---

## Backstory

This repo is maintained by **lobsters/claws**, not by a conventional human-only dev team.

The people behind the system are [Bellman / Yeachan Heo](https://github.com/Yeachan-Heo) and friends like [Yeongyu](https://github.com/code-yeongyu), but the repo itself is being pushed forward by autonomous claw workflows: parallel coding sessions, event-driven orchestration, recovery loops, and machine-readable lane state.

In practice, that means this project is not just *about* coding agents — it is being **actively built by them**. Features, tests, telemetry, docs, and workflow hardening are landed through claw-driven loops using [clawhip](https://github.com/Yeachan-Heo/clawhip), [oh-my-openagent](https://github.com/code-yeongyu/oh-my-openagent), [oh-my-claudecode](https://github.com/Yeachan-Heo/oh-my-claudecode), and [oh-my-codex](https://github.com/Yeachan-Heo/oh-my-codex).

This repository exists to prove that an open coding harness can be built **autonomously, in public, and at high velocity** — with humans setting direction and claws doing the grinding.

See the public build story here:

https://x.com/realsigridjin/status/2039472968624185713

![Tweet screenshot](assets/tweet-screenshot.png)

---

## Implementation Status

The active implementation is the Rust workspace under [`rust/`](./rust).

- `rust/` contains the maintained `claw` CLI, runtime, tool registry, mock parity harness, and verification surface.
- `src/` and `tests/` remain in the repository as reference and validation surfaces that should stay aligned with the documented behavior.
- [`USAGE.md`](./USAGE.md) is the best starting point for build, auth, CLI, session, and parity-harness workflows.

## Recent Hardening

The current patch set tightened the live runtime in four places:

- **Centralized permission enforcement** across built-ins and plugin tools, using one policy surface derived from each tool's declared required permission.
- **Workspace-bounded file mutation** for `write_file`, `edit_file`, and notebook edits, so `workspace-write` mode cannot mutate paths outside the active workspace.
- **CLI dispatch enforcement** for `ToolSearch` and dynamically registered runtime/MCP tools before those branches execute.
- **Temp-dir hardening** for config and tool fixtures so full-workspace verification stays stable under parallel execution.

## Repository Layout

```text
.
├── rust/                               # Active Rust workspace and `claw` CLI
│   ├── Cargo.toml
│   ├── README.md
│   ├── USAGE.md
│   ├── PARITY.md
│   ├── MOCK_PARITY_HARNESS.md
│   ├── crates/
│   └── scripts/
├── src/                                # Reference and compatibility sources kept in-repo
├── tests/                              # Validation surfaces kept in sync with repo behavior
├── assets/                             # README and workflow assets
├── PHILOSOPHY.md
├── ROADMAP.md
├── USAGE.md
├── PARITY.md
└── README.md
```

## Quickstart

Build the active Rust workspace:

```bash
cd rust
cargo build --workspace
```

Run the interactive CLI:

```bash
cd rust
./target/debug/claw
```

Run a one-shot prompt:

```bash
cd rust
./target/debug/claw prompt "summarize this repository"
```

Run the verification sweep:

```bash
cd rust
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
```

Run the mock parity harness:

```bash
cd rust
./scripts/run_mock_parity_harness.sh
```

## Current Runtime Notes

- Permission checks now cover built-ins, plugin tools, `ToolSearch`, and runtime/MCP tool definitions consistently.
- `workspace-write` mode allows mutation only inside the active workspace; absolute paths and missing-target traversal are still rejected when they escape the workspace root.
- The mock parity harness exercises allow/deny flows for file tools, bash permission prompts, multi-tool turns, and plugin tool roundtrips.
- The detailed behavior and remaining parity notes live in [`PARITY.md`](./PARITY.md) and [`rust/PARITY.md`](./rust/PARITY.md).


## Built with `oh-my-codex`

The restructuring and documentation work on this repository was AI-assisted and orchestrated with Yeachan Heo's [oh-my-codex (OmX)](https://github.com/Yeachan-Heo/oh-my-codex), layered on top of Codex.

- **`$team` mode:** used for coordinated parallel review and architectural feedback
- **`$ralph` mode:** used for persistent execution, verification, and completion discipline
- **Codex-driven workflow:** used to harden the Rust workspace, verification surfaces, and documentation around the active CLI/runtime

### OmX workflow screenshots

![OmX workflow screenshot 1](assets/omx/omx-readme-review-1.png)

*Ralph/team orchestration view while the README and essay context were being reviewed in terminal panes.*

![OmX workflow screenshot 2](assets/omx/omx-readme-review-2.png)

*Split-pane review and verification flow during the final README wording pass.*

## Community

<p align="center">
  <a href="https://discord.gg/6ztZB9jvWq"><img src="https://img.shields.io/badge/UltraWorkers-Discord-5865F2?logo=discord&style=for-the-badge" alt="UltraWorkers Discord" /></a>
</p>

Join the [**UltraWorkers Discord**](https://discord.gg/6ztZB9jvWq) — the community around clawhip, oh-my-openagent, oh-my-claudecode, oh-my-codex, and claw-code. Come chat about LLMs, harness engineering, agent workflows, and autonomous software development.

[![Discord](https://img.shields.io/badge/Join%20Discord-UltraWorkers-5865F2?logo=discord&style=for-the-badge)](https://discord.gg/6ztZB9jvWq)

## Star History

See the chart at the top of this README.

## Ownership / Affiliation Disclaimer

- This repository does **not** claim ownership of the original Claude Code source material.
- This repository is **not affiliated with, endorsed by, or maintained by Anthropic**.
