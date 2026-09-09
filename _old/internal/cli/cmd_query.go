package cli

import (
	"context"
	"fmt"
	"os"
	"time"

	"github.com/bgunnarsson/binsql/internal/render"
	"github.com/bgunnarsson/binsql/internal/sqlutil"
)

func queryCommand() *command {
	return &command{
		name:    "query",
		aliases: []string{"q", "select"},
		summary: "Run a read-only query and print the result set",
		usage: `binsql query — run a read-only query

USAGE
  binsql query [flags] "<sql>"
  binsql query [flags] -f query.sql
  cat query.sql | binsql query [flags]

FLAGS
  -f, --file FILE     read the SQL from a file ("-" for stdin)
      --arg VALUE     bind one "?" placeholder; repeat in order
      --allow-write   permit a statement that modifies the database
                      (prefer "binsql exec")

  plus the shared connection and output flags — see "binsql help".

BIND ARGUMENTS
  Write placeholders as "?" regardless of driver; they are rewritten to
  $1 for postgres and @p1 for SQL Server. Values are strings unless
  prefixed with a type:

    --arg int:42  --arg float:1.5  --arg bool:true  --arg null:  --arg str:7

EXAMPLES
  binsql query "select id, email from users order by id limit 20"
  binsql query "select * from orders where customer_id = ?" --arg int:31 -o json
  binsql query "select count(*) from events" -o raw
`,
		run: runQuery,
	}
}

func runQuery(ctx context.Context, argv []string) error {
	var (
		common     commonFlags
		file       string
		args       argList
		allowWrite bool
	)

	fs := newFlagSet("query", "")
	common.register(fs, 30*time.Second)
	fs.StringVar(&file, "file", "", "read SQL from a file")
	fs.StringVar(&file, "f", "", "read SQL from a file (shorthand)")
	fs.Var(&args, "arg", "bind value for a ? placeholder (repeatable)")
	fs.BoolVar(&allowWrite, "allow-write", false, "permit statements that modify the database")
	if err := parseFlags(fs, argv); err != nil {
		return usageError{err}
	}

	sql, err := readSQL(fs.Args(), file)
	if err != nil {
		return err
	}

	// Resolve and vet the SQL before opening a connection, so a rejected
	// statement never reaches the network.
	p, err := common.plan(ctx)
	if err != nil {
		return err
	}

	statements := sqlutil.Split(sql, p.sqlOpts)
	if len(statements) == 0 {
		return usagef("no SQL statement found")
	}
	if len(statements) > 1 {
		return usagef("`query` runs a single statement; got %d — use `binsql exec` for scripts", len(statements))
	}
	if err := p.guard(statements); err != nil {
		return err
	}

	stmt := statements[0]
	if stmt.Kind.Mutates() && !allowWrite {
		return fmt.Errorf(
			"refusing to run a %s statement with `query`: use `binsql exec` (or pass --allow-write)\n  statement: %s",
			stmt.Kind, sqlutil.Summarize(stmt.SQL, p.sqlOpts, 80))
	}

	sess, err := p.connect(ctx)
	if err != nil {
		return err
	}
	defer sess.close()

	bound, err := sess.bind(stmt.SQL, args)
	if err != nil {
		return err
	}

	qctx, cancel := sess.context(ctx)
	defer cancel()

	start := time.Now()
	rows, err := sess.db.Query(qctx, bound, args...)
	if err != nil {
		return queryError(err, qctx, sess.timeout)
	}
	elapsed := time.Since(start)

	return render.Rows(os.Stdout, render.Result{
		Rows:       rows,
		DurationMS: float64(elapsed.Microseconds()) / 1000,
	}, sess.opts)
}

// queryError adds a hint when the failure was our own timeout rather than
// something the database reported.
func queryError(err error, ctx context.Context, timeout time.Duration) error {
	if ctx.Err() == context.DeadlineExceeded {
		return fmt.Errorf("timed out after %s (raise or disable it with --timeout): %w", timeout, err)
	}
	return err
}
