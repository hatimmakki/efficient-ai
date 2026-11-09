# Read & Turn Discipline (H3, H4)

- Batch independent tool calls in a single turn.
- For files >200 lines, inspect signatures (`outline <path>`) or targeted line ranges before whole-file reads.
- After search, read only the enclosing symbol or matched range, not the whole file.
- Stop immediately once verification passes; do not re-run passing checks.
