# Getting started

## Install

```bash
cargo install rustqual
```

Or from source:

```bash
git clone https://github.com/SaschaOnTour/rustqual
cd rustqual
cargo install --path .
```

You can run rustqual two ways — they're equivalent:

```bash
rustqual                 # direct invocation
cargo qual               # as a cargo subcommand
```

## First run

```bash
cd your-rust-project
rustqual
```

By default rustqual analyses `.`, prints a coloured summary, and exits with code `1` if it found anything. For local exploration that shouldn't fail:

```bash
rustqual --no-fail
```

## What you'll see

```
── src/order.rs
  ✓ INTEGRATION process_order (line 12)
  ✓ OPERATION   calculate_discount (line 28)
  ✗ VIOLATION   process_payment (line 48) [MEDIUM]

═══ Summary ═══
  Functions: 24    Quality Score: 82.3%

  IOSP:           85.7%
  Complexity:     90.0%
  DRY:            95.0%
  SRP:           100.0%
  Test Quality:  100.0%
  Coupling:      100.0%
  Architecture:  100.0%

4 quality findings. Run with --verbose for details.
```

Each function is classified as **Integration** (orchestrates other functions, no logic), **Operation** (logic, no own calls), **Violation** (mixes both — the smell to fix), or **Trivial** (too small to matter).

`--verbose` shows every function plus its complexity metrics. `--findings` prints one location per line, useful for piping to `grep`.

## Generate a config

```bash
rustqual --init
```

This writes a `rustqual.toml` next to `Cargo.toml`, with thresholds calibrated to your current codebase metrics. You can edit any section to tighten or relax checks. Full reference: [reference-configuration.md](./reference-configuration.md).

## Where to go next

- **Building with AI assistants?** → [ai-coding-workflow.md](./ai-coding-workflow.md)
- **Adopting on a large existing codebase?** → [legacy-adoption.md](./legacy-adoption.md)
- **Setting up CI?** → [ci-integration.md](./ci-integration.md)
- **Specific quality concerns?** → see the use-case files in this directory:
  - [function-quality.md](./function-quality.md) — IOSP, complexity, length, magic numbers
  - [module-quality.md](./module-quality.md) — module size, cohesion, function clusters
  - [coupling-quality.md](./coupling-quality.md) — circular deps, instability, coupling drift
  - [code-reuse.md](./code-reuse.md) — duplicates, dead code, boilerplate
  - [test-quality.md](./test-quality.md) — assertions, coverage, untested functions
  - [architecture-rules.md](./architecture-rules.md) — layers, forbidden edges, trait contracts
  - [adapter-parity.md](./adapter-parity.md) — keep adapter layers in sync

## Common flags worth knowing

| Flag | Use |
|---|---|
| `--no-fail` | Local exploration; don't exit non-zero |
| `--verbose` | Show every function, not just findings |
| `--findings` | One finding per line: `file:line category in fn_name` |
| `--diff [REF]` | Only analyse files changed vs a git ref |
| `--coverage <LCOV>` | Include coverage-based test-quality checks |
| `--init` | Generate a config tailored to your codebase |
| `--watch` | Re-analyse on file changes |
| `--explain <FILE>` | Architecture diagnostic for one file |

Full flag list: [reference-cli.md](./reference-cli.md).
