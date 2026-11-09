---
name: outline
description: Inspect code file symbol signatures (`[Lstart-Lend]`), read single symbols, and search with enclosing-symbol resolution.
---

# `outline` — Structure-Aware File & Symbol Reader (Rust)

File reading accounts for ~64% of tool-output tokens. Use the `outline` Rust CLI on files >200 lines to inspect signatures and line ranges (`−89.7%` tokens) or read a single function/class (`−72.7%` tokens) instead of loading the entire file into context.

## Commands

```bash
# 1. List all classes, structs, functions, and methods with [Lstart-Lend] line ranges
outline path/to/file.py

# 2. Read the full implementation of a specific symbol
outline path/to/file.py --symbol function_name

# 3. Read a specific 1-based line range
outline path/to/file.py --lines 100-160

# 4. Search across files and group matches by enclosing symbol
outline search "pattern" src/
outline search "pattern" src/ --expand
```
