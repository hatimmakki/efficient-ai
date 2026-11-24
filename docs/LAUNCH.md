# `efficient-ai` v1.0.0 — Release & Launch Playbook

## 1. Cut & Publish the `v1.0.0` GitHub Release (Triggers Prebuilt Binaries)

```bash
git push origin master
git tag -a v1.0.0 -m "efficient-ai v1.0.0: unified eai binary, 4-agent plugins, MCP, dashboard & cache-aware proxy"
git push origin v1.0.0
```

Once `.github/workflows/release.yml` finishes (~3 min), grab `SHA256SUMS.txt` from the GitHub Release to update:
- [`packaging/homebrew/eai.rb`](../packaging/homebrew/eai.rb) (push to `hatimmakki/homebrew-tap`)
- [`packaging/scoop/eai.json`](../packaging/scoop/eai.json) (push to Scoop bucket)
- `cd packages/cli && npm publish --access public` (unlocks `npx efficient-ai analyze` & `npx efficient-ai init`)

Enable GitHub Pages (`Settings -> Pages -> Source: GitHub Actions`) so `.github/workflows/pages.yml` publishes [`docs/index.html`](index.html) live.

---

## 2. Plugin Marketplace & Registry Submissions

| Ecosystem | Submission / Install Command |
|---|---|
| **Claude Code Marketplace** | `/plugin marketplace add hatimmakki/efficient-ai` & `/plugin install efficient-ai@efficient-ai` |
| **Antigravity Plugin Registry** | `agy plugin install https://github.com/hatimmakki/efficient-ai` (native `.antigravity-plugin/plugin.json` + root `hooks.json`) |
| **Gemini CLI Extensions** | `gemini extensions install https://github.com/hatimmakki/efficient-ai` |
| **Codex CLI Marketplace** | `codex plugin marketplace add https://github.com/hatimmakki/efficient-ai.git` |
| **Universal Agent Skills (`skills.sh`)** | `npx skills add hatimmakki/efficient-ai` |
| **MCP Registries (Smithery / PulseMCP / Awesome-MCP)** | Submit `eai mcp` (~248 idle tokens for all 4 tools: `outline_file`, `outline_search`, `squeeze_run`, `eai_stats`) |

---

## 3. Show HN / Reddit / X Launch Copy (Backed by Real Data)

### Show HN Title
> **Show HN: efficient-ai – Single Rust binary cutting AI coding agent cost 27–38% without busting prompt caches**

### Core Narrative (3 Counter-Intuitive Empirical Proofs)
1. **Why popular "token killer" wrappers backfire (+7.6% higher cost):** They count single-stream bytes stripped from `git`/`pytest` (only ~7% of tool tokens), ignore that **file reads drive ~64% of tool exposure**, and drop error lines that cause recovery loops (~21,300 cost-weighted tokens wasted per extra turn).
2. **Why naive history pruning raises Claude bills by +111.8%:** Anthropic cache writes cost **12.5×** cache reads (`1.25×` vs `0.10×`). Sliding-window per-turn masking invalidates the prefix every turn. Our **Cache-Aware Batch Masking (`eai proxy`, $K=12, M=6$)** compacts only at fixed boundaries, cutting Claude cost **−10.5%** and Codex cost **−21.0%**.
3. **Zero-config, 100% local, works on every agent:** Run `eai analyze` (0 config changes) to see real spend across **Claude Code, Antigravity, Gemini CLI, and Codex CLI**, `eai init` (670ms setup), `eai dashboard` (offline local web UI), and `eai share` for a privacy-sanitized SVG proof card.

### Repo Badge for Adopters
```markdown
[![efficient-ai](https://img.shields.io/badge/efficient--ai-same%20quality%2C%20fewer%20tokens-5B54FA)](https://github.com/hatimmakki/efficient-ai)
```
