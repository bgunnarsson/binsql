---
title: What does an opt-in statement deadline bound?
kind: grilling
mode: hitl
status: resolved
blocked_by: [3]
claimed_by:
---

## Question

Choose an additive statement deadline for query, exec and inspect reads.
Recommend milliseconds with no deadline added by default. Settle whether a
batch receives a budget per statement or for the whole batch, whether bind
preparation and metadata reads count, and what happens during cancellation,
rollback and reconnect. Keep cleanup bounded and report unknown write outcome
honestly; a timeout is not proof of rollback or permission to retry. Distinguish
execution deadlines from lock/busy timeouts and connect budgets. Cut tasks small
enough to validate adapter-specific behaviour across the four backends.

## Context

Ticket 03, PostgreSQL and sqlcmd timeout sources it cites;
`crates/binsql/src/cli/query.rs`, exec.rs, inspect.rs;
`crates/binsql-core/src/session.rs`, adapter/sqlx_common.rs and adapter/mssql.rs.
Read 10/11 if resolved; preserve their error/connect contracts.

## Answer

`query`, `exec` and `inspect` get one additive flag, `--timeout-ms N`. It is off by default, and `0` also means off. It sets one client-side deadline that covers everything a command does after the connection opens, for the whole batch. When it fires, the command uses the existing cancel path and gets a fixed 2000 ms to clean up, then exits 1 with a timeout message that never claims a rollback.

### Contract

- **Flag**: `--timeout-ms N` is a shared value flag for all three verbs (`SHARED_VALUES`, `crates/binsql/src/cli/mod.rs:138`). N is a non-negative integer of at most 2147483647. Leaving it out, or passing `0`, keeps today's behaviour with no deadline, the same way `--limit 0` means "none" (`query.rs:21-28`). A value that is not a number, or is too large, is a usage error with exit 2. No command mode code sets a deadline today: the only stop is ⌃C (`mod.rs:243-252`).
  - Assumed, not asked: the name is `--timeout-ms` and not `--statement-timeout-ms`. Why: it bounds the whole command after connecting, not each statement, so PostgreSQL's per-statement name would mislead. The HELP text says it excludes connecting.
  - Assumed, not asked: there is no `BINSQL_TIMEOUT_MS` environment variable. Why: it changes least, and one can be added later.
  - Assumed, not asked: the upper limit is INT_MAX ms. Why: it matches PostgreSQL's `statement_timeout` range and avoids `Instant` overflow.
- **Scope: the whole batch, not each statement.** The clock starts when `connect()` returns (`mod.rs:235-237`) and covers every server call after that:
  - pool acquire;
  - PostgreSQL's `pg_backend_pid` probe (`postgres.rs:81-87`) and MySQL's `CONNECTION_ID` probe (`mysql.rs:69-75`);
  - bind preparation, including PostgreSQL's `describe` (`sqlx_common.rs:119-123`, which has no cancel point today);
  - `BEGIN`, each statement and `COMMIT`;
  - lock and busy waits on the server;
  - a per-catalog connection that `inspect --catalog` opens inside a call (`session.rs:90`);
  - every metadata read `inspect` makes (`inspect.rs:48-51, 65-68, 107-110, 158-175`).
  - Not covered: Workspace load, secret and Key Vault resolution, the adapter connect, and the startup version/database probes (`session.rs:36-52`, `postgres.rs:50-57`, `mssql.rs:51-57`). Those belong to ticket 11's `--connect-timeout-ms`.
  - So the worst-case wall time is the connect time, plus N, plus the cleanup grace.
  - Assumed, not asked: one budget for the whole batch. Why: an agent wants one bound on wall time, and a budget per statement multiplies with the batch length.
- **What happens when it fires.** One internal token is cancelled by either ⌃C or the deadline, and the command records which one fired. The token goes through the existing `run_bound` and `run_transaction` paths (`query.rs:59-63`, `exec.rs:57-91`), so each adapter does what it already does on a cancel:
  - PostgreSQL sends `pg_cancel_backend` (`postgres.rs:92-103`).
  - MySQL sends `KILL QUERY` (`mysql.rs:80-89`).
  - SQL Server drops the connection and reconnects (`mssql.rs:109-112`).
  - SQLite drops the stream (`sqlx_common.rs:145-148`). Whether this stops SQLite's worker is unchecked.
  - `inspect` has no token, so its work is dropped at the deadline, with no grace.
- **Cleanup is bounded.** After the deadline, the work gets a fixed grace of 2000 ms. When the grace runs out the command drops the work and exits, which closes the sockets. This grace is needed because these steps can otherwise wait forever:
  - the rollback after an error (`sqlx_common.rs:71`, its result discarded);
  - `COMMIT` (`sqlx_common.rs:77-78`, which does not watch the token);
  - SQL Server's `ROLLBACK` and `COMMIT`, sent with fresh tokens (`mssql.rs:457-458, 465`);
  - SQL Server's reconnect (`mssql.rs:110`);
  - SQL Server's statement send (`mssql.rs:166-174`);
  - the second connection PostgreSQL and MySQL may open to send the cancel.
  - Assumed, not asked: the grace is a fixed 2000 ms and applies only to deadline cancels. Why: without the flag, ⌃C keeps today's behaviour.
- **Reconnect and retry.** binsql never retries a statement and never reconnects for the caller. SQL Server's own reconnect after a cancel happens inside the grace and changes nothing in the outcome. A reconnect failure, or a server "query cancelled" error, that shows up after the deadline is reported as the timeout, with that error as detail. Today the SQL Server reconnect error replaces the cancel (`mssql.rs:106-111`).
- **If the work finishes inside the grace,** the result stands: output is printed and the exit code is 0. A completed commit is reported as a success.
- **The timeout result.** Exit code is 1, stdout is empty, and rows collected so far are thrown away (partial output is ticket 13's). stderr starts with `error: timed out after N ms`, followed by what is known. A timeout never says "rolled back" or "nothing was kept":
  - **exec without a transaction**: "statement K of M was running; whether it took effect is unknown". When K > 1, the existing wording about earlier statements that already ran and were not rolled back is kept (`exec.rs:76-82`). No statement is started once the deadline has passed.
  - **exec in a transaction**: "the transaction did not report a commit; binsql sends no COMMIT after the deadline, but one already in flight may still land — check before retrying". This replaces, for timeouts only, the message at `exec.rs:62-66`, which claims a rollback.
  - **query and inspect**: "nothing was changed by binsql". Whether the server stopped is unchecked.
  - When ticket 10 settles, the timeout gets its own structured category, separate from ⌃C and from ticket 11's connect timeout.
- **Lock, busy and server timeouts are separate.** The flag sets no server session setting. That means none of PostgreSQL's `statement_timeout` or `lock_timeout`, MySQL's `max_execution_time` or `innodb_lock_wait_timeout`, SQLite's `busy_timeout` or SQL Server's `LOCK_TIMEOUT`. binsql's SQLite connect does not set `busy_timeout` (`sqlite.rs:50-58`), so sqlx's default applies (unchecked).
  - A lock wait counts against `--timeout-ms` because it happens inside the execution phase.
  - A server-side timeout that fires first is a database error, with exit 1 and today's message, not this timeout.
  - Assumed, not asked: the deadline is client-side and not mapped onto server settings. Why: the server settings work per statement, cover different things on each backend (MySQL's covers only SELECT), and change session state.
- **COMMIT once cancelled.** In core, `run_transaction` checks the token before it sends `COMMIT`. If the token is cancelled, it rolls back and returns `Cancelled`. Today it commits anyway (`sqlx_common.rs:77`, `mssql.rs:464`).
  - Assumed, not asked: this also applies to ⌃C. Why: a stop request that arrives before `COMMIT` should not commit.
- **Unchecked**, with each item assigned to a backend ticket below:
  - Inside a transaction, PostgreSQL and MySQL send no server-side cancel (`postgres.rs:284-287` goes straight to `sqlx_common::run_transaction`). The statement may keep running until the socket closes.
  - It is not known whether sqlx's rollback on a stream that was dropped part-way waits for the running statement.
  - It is not known whether a SQLite step stops when its stream is dropped.
  - It is not known how quickly SQL Server rolls back after a disconnect.
  - It is not known whether the tiberius send blocks on a lock.

### Build tickets

**60 — `--timeout-ms` for query, with a bounded cancel**
- blocked_by: []
- Add `timeout-ms` to `SHARED_VALUES` (`mod.rs:138`), and a parser for it: absent or 0 means off, a value that is not a number or is above 2147483647 is exit 2.
- Add a stop helper in `cli/mod.rs` to replace `cancel_on_interrupt`. It holds a token cancelled by ⌃C or by the deadline, records which one fired, and runs the work through a function that waits for the result, or for the deadline plus a grace argument (2000 ms). It reports a completed result as success, and a deadline that fired as the `timed out after N ms` failure (exit 1) with any adapter error as detail.
- `query` uses the helper (`query.rs:59-63`). The clock starts after `connect` returns.
- Tests: argument parsing (absent, 0, a number, a value that is not a number, a value that is too large); without the flag, the output is byte-for-byte unchanged; on SQLite, an endless recursive CTE with `--timeout-ms 200` exits 1 within the timeout plus the grace, with empty stdout and the stderr prefix.
- Update the README and HELP: the flag excludes connecting, the cleanup grace is 2 s, and a timeout is not proof of rollback.

**61 — exec under the deadline: whole batch, honest messages**
- blocked_by: [60, 63]
- `exec` runs both the transactional and the non-transactional paths through the stop helper. The non-transactional loop checks the token before each statement and never starts one after the deadline.
- Timeout messages follow the contract above. The existing ⌃C and error messages are unchanged (`exec.rs:62-66, 74-86`).
- Tests on SQLite: a batch whose second statement runs too long reports "statement 2 of N" and "1 earlier statement already ran". A transactional batch that times out leaves the table unchanged, and its message does not contain "rolled back". `--dry-run` with the deadline does the same.

**62 — inspect under the deadline**
- blocked_by: [60]
- Wrap `list` and `describe` (`inspect.rs:34-37`) in the stop helper with a grace of 0. The metadata reads keep their current signatures and are dropped at the deadline.
- Tests: on SQLite, a large timeout gives output byte-for-byte the same as without the flag; the timeout path gives exit 1 with empty stdout.
- Coordinate with 25 and 26, which also edit `inspect.rs`, so `--columns` reads get the same treatment.
- Update the README and HELP to say the flag applies to inspect.

**63 — No COMMIT after a cancel**
- blocked_by: []
- `sqlx_common::run_transaction` (`sqlx_common.rs:77`) and the SQL Server `run_transaction` (`mssql.rs:464`) check `cancel.is_cancelled()` before `COMMIT`. If it is cancelled, they roll back (best-effort) and return `Error::Cancelled`.
- Tests: a core test on SQLite with a token cancelled after the last statement and before commit; the rows are not present afterwards.

**64 — PostgreSQL and MySQL stop the server statement inside a transaction**
- blocked_by: []
- Give the transaction path the same backend-pid / connection-id capture and `pg_cancel_backend` / `KILL QUERY` that the single-statement path has (`postgres.rs:75-106`, `mysql.rs:57-92`). For example, `run_transaction` takes an optional "on cancel" hook from the adapter.
- Tests: live-server tests on each backend, gated the way the repo gates live tests (unchecked whether such gating exists): `pg_sleep(30)` / `SLEEP(30)` inside `--tx`, cancelled; `pg_stat_activity` / `PROCESSLIST` shows no running statement within the grace.

**65 — Check PostgreSQL behaviour under `--timeout-ms`**
- blocked_by: [60, 61, 64]
- Live tests:
  - `pg_sleep` in query;
  - a lock wait on a row held by a second session;
  - a timeout during a PostgreSQL `describe` (bound `--arg`);
  - `inspect --catalog <other>`, which counts the per-catalog connect.
- Each exits within N plus 2 s, with no running statement left behind and no row committed.
- Record the results in the ticket, and correct the README if the outcome text overstates anything.

**66 — Check MySQL behaviour under `--timeout-ms`**
- blocked_by: [60, 61, 64]
- Live tests: `SLEEP` in query and in exec; an InnoDB lock wait shorter than `innodb_lock_wait_timeout`; DDL inside a timed-out `--tx` batch.
- The message must not suggest the DDL was undone, since MySQL commits it implicitly (`exec.rs:136-154`).
- Record the results.

**67 — Check SQL Server behaviour under `--timeout-ms`**
- blocked_by: [60, 61, 63]
- Live tests:
  - `WAITFOR DELAY` in query and in exec, with and without a transaction;
  - a lock wait;
  - whether `simple_query` / `query` send blocks before the stream returns (`mssql.rs:166-174`);
  - a slow reconnect after a cancel (for example a fedauth source), which must not go past the grace.
- Check that the server rolls back the open transaction after the disconnect, and record the results.

**68 — Check SQLite behaviour under `--timeout-ms`**
- blocked_by: [60, 61, 63]
- Tests:
  - a write lock held by a second connection, with a busy wait shorter and one longer than sqlx's default `busy_timeout` (unchecked);
  - a long CPU-bound CTE: check whether the worker stops when the stream is dropped or only at process exit, and that the lock is released after exit.
- Record the results. If the step keeps running, document that the deadline is enforced by the process exiting.
