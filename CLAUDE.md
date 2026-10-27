# Notes for Claude

- Read `ROADMAP.md` before starting work on LongCut: it holds every planned
  stage and what is already done. When a pull request finishes or starts a
  stage, update `ROADMAP.md` in that same pull request; when the owner
  plans something new, add it there so it is not forgotten.
- The Cargo workspace is `src/`, not the repository root. Before pushing:
  `cargo fmt --check`, `cargo clippy -p <crate> --all-targets -- -D warnings`,
  `cargo test -p <crate>`, and `python3 scripts/locales.py --check` when
  interface text changed (new keys go in all 25 locales).
