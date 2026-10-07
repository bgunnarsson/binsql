---
title: "SQL Server views return their module text"
kind: task
mode: afk
status: open
blocked_by: [80]
claimed_by:
---

## Question

Build what 16 settled for this step; the contract is in 16's answer
(`16-ddl-output.md`).

- Uses `sys.sql_modules.definition` for views: present gives `Create`, null gives `Withheld`. Tables give `Unsupported`.
- Tests: a unit test of the null-to-`Withheld` mapping and of the kind mapping.
- Updates the README's per-backend row once 84 has added it.

## Answer
