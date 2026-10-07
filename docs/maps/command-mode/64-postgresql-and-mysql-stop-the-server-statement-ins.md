---
title: "PostgreSQL and MySQL stop the server statement inside a transaction"
kind: task
mode: afk
status: resolved
blocked_by: []
claimed_by:
---

## Question

Build what 12 settled for this step; the contract is in 12's answer
(`12-statement-budget.md`).

- Give the transaction path the same backend-pid / connection-id capture and `pg_cancel_backend` / `KILL QUERY` that the single-statement path has (`postgres.rs:75-106`, `mysql.rs:57-92`). For example, `run_transaction` takes an optional "on cancel" hook from the adapter.
- Tests: live-server tests on each backend, gated the way the repo gates live tests (unchecked whether such gating exists): `pg_sleep(30)` / `SLEEP(30)` inside `--tx`, cancelled; `pg_stat_activity` / `PROCESSLIST` shows no running statement within the grace.

## Answer

A cancel inside a PostgreSQL or MySQL transaction now stops the statement on
the server, with `pg_cancel_backend` or `KILL QUERY` from a second pooled
connection, before the rollback is sent, so the rollback no longer queues
behind a statement still running. The single-statement path uses the same
hook, sending what it sent before; SQLite and SQL Server are unchanged.

Built in docs/plans/2026-10-07-transaction-server-cancel.md
(adapter/sqlx_common.rs, adapter/postgres.rs, adapter/mysql.rs,
adapter/sqlite.rs, tests/cancel_servers.rs). The correctness review found
nothing. The security review found no injection or exposure, but that a
session id looked up before `BEGIN` could, behind a transaction-pooling proxy
such as PgBouncer or ProxySQL, name a server session other than the batch's
and cancel someone else's statement; fixed by looking it up after `BEGIN`.
The single-statement path keeps that weakness, as it had before this ticket:
its lookup and its statement are separate autocommit transactions. The
review also noted that a stop waits for a free pooled connection; `exec`
runs one batch, so the pool cannot be full there, and the grace bounds it
anyway; accepted.

The live tests (`BINSQL_TEST_POSTGRES`, `BINSQL_TEST_MYSQL`) were not run:
no server was available. 65 and 66 run them.

Assumed, not asked:
- The hook is `Interrupt { identify, stop }` on the codec: function pointers returning boxed futures, so core needs no decode bounds per backend.
- The single-statement path moves onto the hook, so the two paths cannot drift apart; what it sends is unchanged.
- The session is named after `BEGIN`; a lookup that fails leaves the batch without a server stop, as it does for a single statement.
- The stop is sent only for a statement the cancel interrupted, and before the rollback; a cancel after the last statement has nothing to stop.
- The live tests are ignored and gated on the environment, as the keychain test is.
