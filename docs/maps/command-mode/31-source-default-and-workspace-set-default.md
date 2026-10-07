---
title: "`source default` and `Workspace::set_default`"
kind: task
mode: afk
status: resolved
blocked_by: [30]
claimed_by:
---

## Question

Build what 08 settled for this step; the contract is in 08's answer
(`08-data-source-commands.md`).

- Write `default` into the source's file or the `--scope` file. Refuse with exit 1 when the project file's default would win.
- Tests in the core for user and project scope and the shadowing refusal. A CLI test that `source default` with no name prints the current default.
- HELP and README.

## Answer

Built: `binsql source default [NAME] [--scope user|project]` and `Workspace::set_default`; plan docs/plans/2026-10-07-source-default.md.

- `Workspace::set_default(id, scope)` (`crates/binsql-core/src/workspace.rs`) writes the qualified id into the source's own file, or the one `scope` names, and rebuilds the merged view. A user write while the project file's default names a different source is refused with a hint to pass `--scope project`; an ephemeral or unknown id, and `--scope project` with no project file, are refused.
- `binsql source default` prints the default's row, or nothing on stdout when none is set (a default naming nothing gets a stderr note). With a NAME it resolves as `show` does (exit 2 if unknown), sets it (core refusals exit 1) and prints the row. `--scope` on any other op, a bad `--scope` value and two names are exit 2.
- Assumed, not asked: an ephemeral source cannot be made the default, even with `--scope` — the next run would find the default naming nothing.
- Assumed, not asked: a project default already naming the same source does not block a user write.
- Pre-existing, unfixed: `Config::save_to` with `Visibility::Shared` replaces a project file through a temp file with the umask's mode, so a project file someone made 0600 becomes 0644 on any write (`set`, `remove`, and now `set_default`).
