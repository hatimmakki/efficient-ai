# Experiment 009: Turnkey Live Paired Agent A/B Harness

Reproducible **paired quality non-inferiority + cost evaluation** comparing **Control** (`eai pause`) vs. **Treatment** (`eai init`) across identical >250-line coding tasks with automated pass/fail unit-test graders and per-project `eai analyze --project <ws> --json` billing reconciliation.

## Usage

```bash
# 1. Verify all starter tasks fail, `eai outline` resolves symbols, and reference solutions pass (0 API spend):
python experiments/009-live-paired-ab/run_paired_ab.py --dry-run

# 2. Run a live paired A/B benchmark against any installed agent CLI:
python experiments/009-live-paired-ab/run_paired_ab.py --agent claude
python experiments/009-live-paired-ab/run_paired_ab.py --agent antigravity
python experiments/009-live-paired-ab/run_paired_ab.py --agent gemini
python experiments/009-live-paired-ab/run_paired_ab.py --agent codex
```
