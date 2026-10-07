---
title: "`source add` and `source edit`"
kind: task
mode: afk
status: resolved
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

binsql source add and source edit save a data source from the command line, filing a connection string in the credential store by default.

Built in docs/plans/2026-10-07-source-add-edit.md (crates/binsql/src/cli/source.rs: Change, add, edit, commit). A connection string comes by --dsn-stdin or --dsn-env; --dsn takes only a sqlite path or a secret reference. A typed string goes to the credential store unless --no-keychain, and is never written literally into the project file. Review fixes: a keychain:// reference is refused however it is given, an id that collides with a folder or a top-level source is refused, and a keyword string ending in .db is not taken for a sqlite path.

Assumed, not asked: on a rename with a newly typed connection string the old keychain entry stays — it is what ticket 20's rename already does, and removing it here would widen the change.
