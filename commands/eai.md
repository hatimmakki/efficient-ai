---
description: Manage efficient-ai (status, update, statusline, pause/resume, impact, audit, doctor)
argument-hint: "[status | update | statusline on|off | pause | resume | impact | audit | doctor]"
---

Handle the user's `/eai` request cleanly using the local `eai` binary:

- If `$ARGUMENTS` is empty or `status`: run `eai doctor` and `eai stats` and present a concise summary (including any available update).
- If `$ARGUMENTS` starts with `update`: run `eai update` (or `eai update --check` if they passed `check`), then report the upgraded version and plugin refresh commands.
- If `$ARGUMENTS` starts with `statusline`: run `eai statusline install` (for `on`/`install`), `eai statusline off` (for `off`), or `eai statusline preview`.
- If `$ARGUMENTS` is `pause` or `resume`: run `eai pause` or `eai resume`.
- If `$ARGUMENTS` starts with `impact`: run `eai impact --project . --since 30d`.
- If `$ARGUMENTS` starts with `audit`: run `eai audit . --include-user`.
- If `$ARGUMENTS` starts with `doctor`: run `eai doctor`.
