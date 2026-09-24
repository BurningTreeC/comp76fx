## Development Environment

This repository is developed on Linux (Arch Linux).

A comprehensive set of development and research tools is installed.
Prefer these tools over slower/basic alternatives whenever appropriate.

### Code and file search

- `rg` / ripgrep
  - Preferred over `grep -R`.
  - Use for searching source code, configuration, logs, and documentation.

- `rga` / ripgrep-all
  - Use for searching PDFs, archives, Office documents, ebooks, etc.

- `fd`
  - Preferred over `find` for locating files.

- `fzf`
  - Available for fuzzy searching.

- `bat`
  - Available for source/text inspection.

- `tree`
  - Use to inspect repository structure.

### Structured data

- `jq`
  - JSON inspection/transformation.

- `yq`
  - YAML inspection/transformation.

### Git and GitHub

- `git`

- `gh`
  - GitHub CLI is installed and authenticated.
  - Use it to inspect:
    - GitHub Actions
    - failed CI runs
    - workflow logs
    - pull requests
    - issues
    - releases
    - repository metadata

Examples:

    gh run list
    gh run view <id>
    gh run view <id> --log-failed
    gh pr list
    gh issue list

### Rust

Available Rust tools include:

- `cargo`
- `rustc`
- `rustfmt`
- `cargo clippy`
- `cargo nextest`
- `cargo audit`
- `cargo expand`
- `cargo bloat`
- `cargo deny`
- `cargo semver-checks`

Prefer:

    cargo nextest run

for normal test suites where appropriate.

Always run:

    cargo fmt --check
    cargo clippy --all-targets --all-features

when appropriate before declaring work complete.

### Build tools

Available:

- `cmake`
- `ninja`
- `pkg-config`
- `clang`
- `llvm`
- `mold`
- `sccache`

### Debugging

Available:

- `gdb`
- `lldb`
- `strace`
- `lsof`

Use `strace` when investigating:
- missing files
- library loading
- filesystem access
- process spawning
- environment-dependent Linux failures

Use `gdb` or `lldb` for native crashes.

### Performance

Available:

- `perf`
- `hyperfine`

For realtime DSP/performance problems, use measurements rather than guessing.

Useful commands include:

    perf stat <command>
    perf record <command>
    perf report

and:

    hyperfine '<command>'

### Shell and CI

Available:

- `shellcheck`
- `shfmt`
- `actionlint`
- `act`

Validate shell scripts with:

    shellcheck <script>

Validate GitHub Actions workflows with:

    actionlint

Use `act` where suitable to reproduce GitHub Actions locally.

### Documentation and research

Available:

- `pdftotext`
- `pdfinfo`
- `pdftoppm`
- `pandoc`
- `w3m`
- `curl`
- `wget`
- `http`

When documentation exists as PDF, prefer extracting its text rather than ignoring it.

Examples:

    pdftotext manual.pdf -
    pdfinfo manual.pdf

### Python

`uv` is installed.

Do not install Python dependencies into the system Python environment.

Use temporary/project environments via `uv`, for example:

    uv run ...
    uv venv
    uv pip install ...

## Tool Usage Policy

Before implementing a complex workaround, check whether an installed tool
already provides the required functionality.

Prefer:

- `rg` instead of recursive grep
- `fd` instead of complex `find`
- `jq` / `yq` instead of parsing structured data manually
- `gh` instead of asking the user to copy GitHub CI logs
- `cargo nextest` for large Rust test suites
- `shellcheck` for shell scripts
- `actionlint` for GitHub Actions
- `hyperfine` / `perf` for performance claims
- `pdftotext` for PDF research

When unsure whether a tool is installed, check with:

    command -v <tool>

and inspect usage with:

    <tool> --help

Do not assume a command is unavailable without checking first.


