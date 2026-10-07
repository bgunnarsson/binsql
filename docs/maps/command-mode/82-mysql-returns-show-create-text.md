---
title: "MySQL returns `SHOW CREATE` text"
kind: task
mode: afk
status: resolved
blocked_by: [80]
claimed_by:
---

## Question

Build what 16 settled for this step; the contract is in 16's answer
(`16-ddl-output.md`).

- Uses `SHOW CREATE TABLE` / `SHOW CREATE VIEW` on the quoted identifier, giving `Create`. Reads the `Create Table` or `Create View` column.
- Tests: a unit test quoting a name that contains a backtick and a dot; a unit test choosing the result column by object kind.
- Updates the README's per-backend row once 84 has added it.

## Answer

A MySQL table now gives `create` with the `Create Table` column of
`SHOW CREATE TABLE`, and a view gives `create` with the `Create View` column
of `SHOW CREATE VIEW`. The object is named through `ObjectRef::qualified`
with the MySQL dialect, so a backtick is doubled and a dot stays inside its
identifier. A missing object is the server's own error, passed on as a query
error; no row or a null column gives `withheld`.

Built in docs/plans/2026-10-07-mysql-show-create.md
(adapter/mysql.rs, tests/definitions_servers.rs, README.md). Unit tests
quote a name holding a backtick and a dot, and check that the kind picks
both the statement and its column. An ignored live test creates a table and
a view and checks both and a missing object. Live PostgreSQL, MySQL and SQL
Server unchecked: Docker is not running here.

Neither review found a defect. The correctness review noted that an account
without the SHOW VIEW privilege gets an error from `SHOW CREATE VIEW`, so
`inspect --definitions` fails where 16's `withheld` mentions a missing
permission; 16 leaves permission behaviour unchecked, so it stays as is.

Assumed, not asked:
- The statement names the object through `ObjectRef::qualified` with the MySQL dialect; with no catalog the connection's database is used.
- It is sent prepared through `sqlx::query`; MySQL can prepare `SHOW CREATE TABLE` and `SHOW CREATE VIEW`, so 16's unprepared fallback is not needed.
- The statement and the column are chosen by the object's kind in one function, unit tested without a server.
- A missing object is the server's error as `Error::Query`; no row and a null column give `withheld`.
- The live test is added to the ignored `tests/definitions_servers.rs` and reads `BINSQL_TEST_MYSQL`.
