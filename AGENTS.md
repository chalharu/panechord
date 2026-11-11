# Project conventions

- Define dependency versions and shared settings in the workspace root's
  `Cargo.toml`; member crates should use `dependency.workspace = true`.
  Standalone tools and test fixtures use their own workspace roots.
- Keep documentation concise and practical. Retain usage, constraints and
  provenance; omit repetition, narrative padding and per-run work logs.