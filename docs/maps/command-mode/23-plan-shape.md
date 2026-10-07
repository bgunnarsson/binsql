---
title: What shape does an estimated plan take on the command line?
kind: grilling
mode: hitl
status: open
blocked_by: [17, 22]
claimed_by:
---

## Question

17 recommends estimated plans only, built by binsql from a single classified
statement, with each backend's native payload returned unchanged. Settle:

- where it lives: an `inspect` form or a `query` flag (05 rules out a new
  verb);
- which payload each backend returns (SQLite rows, PostgreSQL JSON or text,
  MySQL JSON, TREE or traditional rows, SQL Server showplan XML) and how it
  maps onto `--format`;
- whether a write may be planned, given its estimated form does not run it
  and `query` refuses writes without `--allow-write`;
- how a missing SQL Server `SHOWPLAN` permission is reported;
- then cut the build ticket.

## Context

- 17's answer and links; 05's namespace decision; 03's survey.

## Answer
