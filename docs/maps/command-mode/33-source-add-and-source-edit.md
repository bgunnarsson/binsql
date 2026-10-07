---
title: "`source add` and `source edit`"
kind: task
mode: afk
status: open
blocked_by: [20, 30]
claimed_by:
---

## Question

Build what 08 settled for this step; the contract is in 08's answer
(`08-data-source-commands.md`).

- Input through `--dsn-stdin`, `--dsn-env` and `--dsn` (reference or sqlite only).
- Flags: `--driver`, `--description`, the readonly and open-on-start pairs, `--scope`, `--rename` and `--no-keychain`.
- Keychain by default through ticket 20's save. Print the saved row.
- Tests:
  - A literal on `--dsn` is exit 2.
  - `add` over an existing name is exit 2.
  - A reserved name is exit 2.
  - A literal with `--no-keychain` and project scope is exit 2.
  - `--scope` moves the source.
  - `edit` keeps the fields not given.
  - Validation errors match ticket 20's function.
  - Keychain writes go through ticket 20's test seam, never the real store.
- HELP and README.

## Answer
