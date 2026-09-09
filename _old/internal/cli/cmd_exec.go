package cli

import (
	"context"
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/bgunnarsson/binsql/internal/db"
	"github.com/bgunnarsson/binsql/internal/render"
	"github.com/bgunnarsson/binsql/internal/sqlutil"
)

func execCommand() *command {
	return &command{
		name:    "exec",
		aliases: []string{"e", "run"},
		summary: "Run statements that modify data or schema",
		usage: `binsql exec — run statements that change the database

USAGE
  binsql exec [flags] "<sql>"
  binsql exec [flags] -f migration.sql
  cat migration.sql | binsql exec [flags]

Multiple statements separated by ";" are supported and run in order.

FLAGS
  -f, --file FILE   read the SQL from a file ("-" for stdin)
      --arg VALUE   bind one "?" placeholder; repeat in order
      --dry-run     run everything inside a transaction, then roll back
      --tx          force a single transaction around all statements
      --no-tx       run each statement on its own (no transaction)
      --force       allow UPDATE/DELETE without WHERE, and DROP/TRUNCATE

  plus the shared connection and output flags — see "binsql help".

TRANSACTIONS
  Two or more statements are wrapped in one transaction by default, so a
  failure part-way through rolls the whole batch back. A single statement
  runs on its own unless --tx or --dry-run is given.

SAFETY
  UPDATE and DELETE without a WHERE clause, and DROP or TRUNCATE, are
  refused unless --force is passed. --dry-run reports what a batch would
  do without committing it — note that MySQL commits DDL implicitly, so a
  dry run there cannot undo schema changes.

EXAMPLES
  binsql exec "insert into users (email) values (?)" --arg a@b.com
  binsql exec "update users set active = 0 where id = ?" --arg int:9
  binsql exec -f migration.sql --dry-run
  binsql exec "delete from sessions" --force
`,
		run: runExec,
	}
}

func runExec(ctx context.Context, argv []string) error {
	var (
		common commonFlags
		file   string
		args   argList
		dryRun bool
		useTx  bool
		noTx   bool
		force  bool
	)

	fs := newFlagSet("exec", "")
	common.register(fs, 0)
	fs.StringVar(&file, "file", "", "read SQL from a file")
	fs.StringVar(&file, "f", "", "read SQL from a file (shorthand)")
	fs.Var(&args, "arg", "bind value for a ? placeholder (repeatable)")
	fs.BoolVar(&dryRun, "dry-run", false, "roll back instead of committing")
	fs.BoolVar(&useTx, "tx", false, "force a single transaction")
	fs.BoolVar(&noTx, "no-tx", false, "do not use a transaction")
	fs.BoolVar(&force, "force", false, "allow unqualified UPDATE/DELETE and DROP/TRUNCATE")
	if err := parseFlags(fs, argv); err != nil {
		return usageError{err}
	}
	if useTx && noTx {
		return usagef("--tx and --no-tx are mutually exclusive")
	}

	sql, err := readSQL(fs.Args(), file)
	if err != nil {
		return err
	}

	// Resolve and vet the batch before opening a connection, so a rejected
	// statement never reaches the network.
	p, err := common.plan(ctx)
	if err != nil {
		return err
	}

	statements := sqlutil.Split(sql, p.sqlOpts)
	if len(statements) == 0 {
		return usagef("no SQL statement found")
	}
	if err := p.guard(statements); err != nil {
		return err
	}
	if !force {
		if err := checkDestructive(statements, p.sqlOpts); err != nil {
			return err
		}
	}
	if len(args) > 0 && len(statements) > 1 {
		return usagef("--arg can only be used with a single statement")
	}

	// A batch is transactional by default; a lone statement is not.
	transactional := len(statements) > 1
	if useTx || dryRun {
		transactional = true
	}
	if noTx {
		if dryRun {
			return usagef("--dry-run needs a transaction, so it cannot be combined with --no-tx")
		}
		transactional = false
	}

	if dryRun {
		warnImplicitCommit(p, statements)
	}

	sess, err := p.connect(ctx)
	if err != nil {
		return err
	}
	defer sess.close()

	ectx, cancel := sess.context(ctx)
	defer cancel()

	start := time.Now()
	summary := render.ExecSummary{Transaction: transactional, DryRun: dryRun}

	runAll := func(ex db.Executor) error {
		for _, stmt := range statements {
			res, err := runStatement(ectx, sess, ex, stmt, args)
			if err != nil {
				return fmt.Errorf("%w\n  statement: %s", err, sqlutil.Summarize(stmt.SQL, sess.sqlOpts, 120))
			}
			summary.Statements = append(summary.Statements, res)
			summary.TotalRowsAffected += res.RowsAffected
		}
		if dryRun {
			return db.ErrRollback
		}
		return nil
	}

	if transactional {
		err = sess.db.InTx(ectx, runAll)
		summary.RolledBack = dryRun && err == nil
	} else {
		err = runAll(sess.db)
	}
	summary.DurationMS = float64(time.Since(start).Microseconds()) / 1000

	if err != nil {
		if transactional {
			// Report the rollback so nobody assumes a partial write landed.
			sess.status("transaction rolled back; no changes were committed")
		}
		return queryError(err, ectx, sess.timeout)
	}

	return render.Exec(os.Stdout, summary, sess.opts)
}

// runStatement dispatches one statement to Query or Exec depending on whether
// it produces rows, so mixed scripts print their SELECT output.
func runStatement(ctx context.Context, sess *session, ex db.Executor, stmt sqlutil.Statement, args []any) (render.StatementResult, error) {
	out := render.StatementResult{
		SQL:  sqlutil.Summarize(stmt.SQL, sess.sqlOpts, 0),
		Kind: string(stmt.Kind),
	}

	bound, err := sess.bind(stmt.SQL, args)
	if err != nil {
		return out, err
	}

	start := time.Now()
	if stmt.Kind == sqlutil.KindRead {
		rows, err := ex.Query(ctx, bound, args...)
		if err != nil {
			return out, err
		}
		out.Rows = rows
		out.RowCount = len(rows.Data)
	} else {
		res, err := ex.Exec(ctx, bound, args...)
		if err != nil {
			return out, err
		}
		out.RowsAffected = res.RowsAffected
		out.HasRowsAffected = res.HasRowsAffected
		out.LastInsertID = res.LastInsertID
	}
	out.DurationMS = float64(time.Since(start).Microseconds()) / 1000

	return out, nil
}

// checkDestructive blocks the statements most likely to be a mistake.
func checkDestructive(statements []sqlutil.Statement, opts sqlutil.Options) error {
	for i, stmt := range statements {
		words := strings.Fields(strings.ToUpper(sqlutil.Summarize(stmt.SQL, opts, 0)))
		if len(words) == 0 {
			continue
		}

		label := ""
		switch words[0] {
		case "UPDATE", "DELETE":
			if !sqlutil.HasWhere(stmt.SQL, opts) {
				label = fmt.Sprintf("%s without a WHERE clause affects every row", words[0])
			}
		case "DROP", "TRUNCATE":
			label = fmt.Sprintf("%s discards data irreversibly", words[0])
		}

		if label != "" {
			return fmt.Errorf("refusing statement %d: %s — pass --force if that is intended\n  statement: %s",
				i+1, label, sqlutil.Summarize(stmt.SQL, opts, 120))
		}
	}
	return nil
}

// warnImplicitCommit flags the case where --dry-run cannot actually undo the
// work: MySQL commits DDL implicitly, so a rollback leaves schema changes in
// place. Postgres and SQL Server both support transactional DDL.
func warnImplicitCommit(p *plan, statements []sqlutil.Statement) {
	if p.sqlOpts.Dialect != "mysql" {
		return
	}
	for _, stmt := range statements {
		if stmt.Kind == sqlutil.KindDDL {
			p.status("warning: mysql commits DDL implicitly — --dry-run cannot roll back schema changes in this batch")
			return
		}
	}
}
