# Contributing to `efficient-ai`

Thank you for contributing to `efficient-ai`! Our goal is rigorous, reproducible, end-to-end token and cost reduction for AI coding agents without losing task quality.

## Ground Rules

1. **Never commit raw transcripts or secrets.**
   - Do not commit files from `~/.claude/`, `~/.codex/`, `~/.gemini/`, `.eai/`, or raw Harbor/SWE-bench run directories.
   - Only commit sanitized aggregate tables or outputs produced by `tokscope sanitize`.
2. **Measure end-to-end.**
   - New compression or prompt techniques must report impact on **total session tokens** and **cost-weighted tokens** (accounting for prompt-cache writes vs. reads), not just single-stream byte counts.
3. **Run the verification suite before opening a PR:**
   ```bash
   # Python tests & multi-agent fixture self-test
   .venv/bin/python -m unittest discover -s tools/tokscope/tests -v
   .venv/bin/tokscope selftest

   # Rust tools
   cargo test --manifest-path tools/outline/Cargo.toml
   cargo test --manifest-path tools/squeeze/Cargo.toml
   ```

## Developer Certificate of Origin (DCO)

By contributing to this repository, you agree that your contributions are licensed under the [Apache-2.0 License](LICENSE). Please sign off your commits with `git commit -s` (`Signed-off-by: Name <email>`).
