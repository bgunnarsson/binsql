package cli

import (
	"context"
	"os"
	"strings"
	"time"

	"github.com/bgunnarsson/binsql/internal/render"
)

func tablesCommand() *command {
	return &command{
		name:    "tables",
		aliases: []string{"ls", "list"},
		summary: "List tables and views",
		usage: `binsql tables — list tables and views

USAGE
  binsql tables [flags]

FLAGS
  --like PATTERN   keep only names containing PATTERN (case-insensitive)

  plus the shared connection and output flags — see ` + "`binsql help`" + `.

EXAMPLES
  binsql tables
  binsql tables --like user -o raw
`,
		run: runTables,
	}
}

func runTables(ctx context.Context, argv []string) error {
	var (
		common commonFlags
		like   string
	)

	fs := newFlagSet("tables", "")
	common.register(fs, 30*time.Second)
	fs.StringVar(&like, "like", "", "filter names by substring")
	if err := parseFlags(fs, argv); err != nil {
		return usageError{err}
	}

	sess, err := common.open(ctx)
	if err != nil {
		return err
	}
	defer sess.close()

	qctx, cancel := sess.context(ctx)
	defer cancel()

	tables, err := sess.db.ListTables(qctx)
	if err != nil {
		return queryError(err, qctx, sess.timeout)
	}
	tables = filterNames(tables, like)

	return render.List(os.Stdout, "table", tables, sess.opts)
}

func describeCommand() *command {
	return &command{
		name:    "describe",
		aliases: []string{"desc", "columns"},
		summary: "Show the columns of one table",
		usage: `binsql describe — show a table's columns

USAGE
  binsql describe [flags] <table>

Accepts a bare or schema-qualified name, e.g. "users" or "public.users".

FLAGS
  the shared connection and output flags — see ` + "`binsql help`" + `.

EXAMPLES
  binsql describe users
  binsql describe public.orders -o json
`,
		run: runDescribe,
	}
}

func runDescribe(ctx context.Context, argv []string) error {
	var common commonFlags

	fs := newFlagSet("describe", "")
	common.register(fs, 30*time.Second)
	if err := parseFlags(fs, argv); err != nil {
		return usageError{err}
	}
	if fs.NArg() != 1 {
		return usagef("describe takes exactly one table name")
	}
	table := fs.Arg(0)

	sess, err := common.open(ctx)
	if err != nil {
		return err
	}
	defer sess.close()

	qctx, cancel := sess.context(ctx)
	defer cancel()

	cols, err := sess.db.DescribeTable(qctx, table)
	if err != nil {
		return queryError(err, qctx, sess.timeout)
	}

	return render.Table(os.Stdout, render.TableSchema{Table: table, Columns: cols}, sess.opts)
}

func schemaCommand() *command {
	return &command{
		name:    "schema",
		summary: "Dump the schema of every table",
		usage: `binsql schema — dump the schema of every table

USAGE
  binsql schema [flags] [table ...]

With no arguments every table is described. This is the fastest way to
give a tool or a person the whole picture in one call.

FLAGS
  --like PATTERN   only tables whose name contains PATTERN

  plus the shared connection and output flags — see ` + "`binsql help`" + `.

EXAMPLES
  binsql schema -o json --pretty
  binsql schema --like order
  binsql schema users orders
`,
		run: runSchema,
	}
}

func runSchema(ctx context.Context, argv []string) error {
	var (
		common commonFlags
		like   string
	)

	fs := newFlagSet("schema", "")
	common.register(fs, 60*time.Second)
	fs.StringVar(&like, "like", "", "filter table names by substring")
	if err := parseFlags(fs, argv); err != nil {
		return usageError{err}
	}

	sess, err := common.open(ctx)
	if err != nil {
		return err
	}
	defer sess.close()

	qctx, cancel := sess.context(ctx)
	defer cancel()

	names := fs.Args()
	if len(names) == 0 {
		names, err = sess.db.ListTables(qctx)
		if err != nil {
			return queryError(err, qctx, sess.timeout)
		}
		names = filterNames(names, like)
	}

	out := make([]render.TableSchema, 0, len(names))
	for _, name := range names {
		cols, err := sess.db.DescribeTable(qctx, name)
		if err != nil {
			// A view or a table we cannot introspect should not abort the dump.
			sess.status("warning: skipping %s: %v", name, err)
			continue
		}
		out = append(out, render.TableSchema{Table: name, Columns: cols})
	}

	return render.Schema(os.Stdout, out, sess.opts)
}

func countCommand() *command {
	return &command{
		name:    "count",
		summary: "Count rows in a table",
		usage: `binsql count — count rows in a table

USAGE
  binsql count [flags] <table>

FLAGS
  --where PREDICATE   count only matching rows (SQL, without "WHERE")

  plus the shared connection and output flags — see ` + "`binsql help`" + `.

EXAMPLES
  binsql count users
  binsql count orders --where "status = 'open'"
`,
		run: runCount,
	}
}

func runCount(ctx context.Context, argv []string) error {
	var (
		common commonFlags
		where  string
	)

	fs := newFlagSet("count", "")
	common.register(fs, 60*time.Second)
	fs.StringVar(&where, "where", "", "predicate without the WHERE keyword")
	if err := parseFlags(fs, argv); err != nil {
		return usageError{err}
	}
	if fs.NArg() != 1 {
		return usagef("count takes exactly one table name")
	}

	sess, err := common.open(ctx)
	if err != nil {
		return err
	}
	defer sess.close()

	query := "SELECT COUNT(*) AS count FROM " + sess.dialect.QuoteTable(fs.Arg(0))
	if strings.TrimSpace(where) != "" {
		query += " WHERE " + where
	}

	qctx, cancel := sess.context(ctx)
	defer cancel()

	start := time.Now()
	rows, err := sess.db.Query(qctx, query)
	if err != nil {
		return queryError(err, qctx, sess.timeout)
	}

	return render.Rows(os.Stdout, render.Result{
		Rows:       rows,
		DurationMS: float64(time.Since(start).Microseconds()) / 1000,
	}, sess.opts)
}

func headCommand() *command {
	return &command{
		name:    "head",
		aliases: []string{"peek", "sample"},
		summary: "Show the first rows of a table",
		usage: `binsql head — show the first rows of a table

USAGE
  binsql head [flags] <table>

FLAGS
  -n, --limit N       how many rows to fetch (default 10)
      --where PRED    filter rows (SQL, without "WHERE")

  plus the shared connection and output flags — see ` + "`binsql help`" + `.

EXAMPLES
  binsql head users
  binsql head orders -n 50 --where "status = 'open'"
`,
		run: runHead,
	}
}

func runHead(ctx context.Context, argv []string) error {
	var (
		common commonFlags
		limit  int
		where  string
	)

	fs := newFlagSet("head", "")
	common.register(fs, 30*time.Second)
	fs.IntVar(&limit, "limit", 10, "number of rows")
	fs.IntVar(&limit, "n", 10, "number of rows (shorthand)")
	fs.StringVar(&where, "where", "", "predicate without the WHERE keyword")
	if err := parseFlags(fs, argv); err != nil {
		return usageError{err}
	}
	if fs.NArg() != 1 {
		return usagef("head takes exactly one table name")
	}

	sess, err := common.open(ctx)
	if err != nil {
		return err
	}
	defer sess.close()

	query := sess.dialect.SelectLimit(sess.dialect.QuoteTable(fs.Arg(0)), strings.TrimSpace(where), limit)

	qctx, cancel := sess.context(ctx)
	defer cancel()

	start := time.Now()
	rows, err := sess.db.Query(qctx, query)
	if err != nil {
		return queryError(err, qctx, sess.timeout)
	}

	return render.Rows(os.Stdout, render.Result{
		Rows:       rows,
		DurationMS: float64(time.Since(start).Microseconds()) / 1000,
	}, sess.opts)
}

// filterNames keeps names containing the pattern, case-insensitively.
func filterNames(names []string, pattern string) []string {
	if pattern == "" {
		return names
	}
	needle := strings.ToLower(pattern)
	out := make([]string, 0, len(names))
	for _, n := range names {
		if strings.Contains(strings.ToLower(n), needle) {
			out = append(out, n)
		}
	}
	return out
}
