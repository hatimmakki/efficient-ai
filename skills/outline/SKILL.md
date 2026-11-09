---
name: outline
description: Inspect code file symbol signatures (`[Lstart-Lend]`), read single symbols, and search with enclosing-symbol resolution.
---

# `outline` — Structure-Aware File & Symbol Reader

File reading accounts for ~64% of tool-output tokens. Use `outline` on files >200 lines to inspect signatures and line ranges (`−89.7%` tokens) or read a single function/class (`−72.7%` tokens) instead of loading the entire file into context.

## Commands

```bash
# 1. List all classes, structs, functions, and methods with [Lstart-Lend] line ranges
python scripts/eai.py outline path/to/file.py

# 2. Read the full implementation of a specific symbol
python scripts/eai.py outline path/to/file.py --symbol function_name

# 3. Read a specific 1-based line range
python scripts/eai.py outline path/to/file.py --lines 100-160

# 4. Search across files and group matches by enclosing symbol
python scripts/eai.py outline search "pattern" src/
```
