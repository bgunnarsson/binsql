package cli

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"os"
	"strconv"
	"strings"
	"time"

	"golang.org/x/term"

	"github.com/bgunnarsson/binsql/internal/config"
	"github.com/bgunnarsson/binsql/internal/db"
	"github.com/bgunnarsson/binsql/internal/drivers"
	"github.com/bgunnarsson/binsql/internal/render"
	"github.com/bgunnarsson/binsql/internal/secrets"
	"github.com/bgunnarsson/binsql/internal/sqlutil"
)

// commonFlags are accepted by every database-touching command.
type commonFlags struct {
	conn   string
	driver string
	dsn    string

	format   string
	maxWidth int
	maxRows  int
	noHeader bool
	pretty   bool

	timeout   time.Duration
	readOnly  bool
	secretTTL time.Duration
}

// newFlagSet builds a flag set that fails quietly, since Main prints usage.
func newFlagSet(name, usage string) *flag.FlagSet {
	fs := flag.NewFlagSet(name, flag.ContinueOnError)
	fs.SetOutput(io.Discard)
	fs.Usage = func() {}
	_ = usage
	return fs
}

// parseFlags parses argv allowing flags and positional arguments to be mixed.
//
// The standard library stops looking for flags at the first positional, which
// would silently ignore the trailing flags in the natural invocation
// `binsql query "select 1" -o json`. Permuting first keeps that working.
func parseFlags(fs *flag.FlagSet, argv []string) error {
	return fs.Parse(permute(fs, argv))
}

// permute reorders argv so every flag precedes every positional argument.
func permute(fs *flag.FlagSet, argv []string) []string {
	var flags, positional []string

	for i := 0; i < len(argv); i++ {
		arg := argv[i]

		// "--" ends flag parsing; everything after it is positional.
		if arg == "--" {
			positional = append(positional, argv[i+1:]...)
			break
		}

		// "-" on its own means stdin, not a flag.
		if len(arg) < 2 || arg[0] != '-' {
			positional = append(positional, arg)
			continue
		}

		flags = append(flags, arg)

		// "--name=value" carries its own value.
		if strings.Contains(arg, "=") {
			continue
		}

		// Otherwise a non-boolean flag consumes the next argument.
		if !isBoolFlag(fs, strings.TrimLeft(arg, "-")) && i+1 < len(argv) {
			i++
			flags = append(flags, argv[i])
		}
	}

	return append(flags, positional...)
}

// isBoolFlag reports whether a registered flag is a boolean, which the flag
// package signals through an optional IsBoolFlag method on the value.
func isBoolFlag(fs *flag.FlagSet, name string) bool {
	f := fs.Lookup(name)
	if f == nil {
		// Unknown flag: assume it stands alone and let Parse report it.
		return true
	}
	bf, ok := f.Value.(interface{ IsBoolFlag() bool })
	return ok && bf.IsBoolFlag()
}

// register wires the shared flags, seeding defaults from the environment so
// an explicit flag always wins over BINSQL_*.
func (c *commonFlags) register(fs *flag.FlagSet, defaultTimeout time.Duration) {
	fs.StringVar(&c.conn, "conn", os.Getenv("BINSQL_CONN"), "saved connection profile")
	fs.StringVar(&c.conn, "c", os.Getenv("BINSQL_CONN"), "saved connection profile (shorthand)")
	fs.StringVar(&c.driver, "driver", os.Getenv("BINSQL_DRIVER"), "sqlite|postgres|mssql|mysql")
	fs.StringVar(&c.driver, "d", os.Getenv("BINSQL_DRIVER"), "driver (shorthand)")
	fs.StringVar(&c.dsn, "dsn", os.Getenv("BINSQL_DSN"), "connection string or file path")
	fs.StringVar(&c.dsn, "D", os.Getenv("BINSQL_DSN"), "connection string (shorthand)")

	defFormat := os.Getenv("BINSQL_FORMAT")
	fs.StringVar(&c.format, "format", defFormat, "output format")
	fs.StringVar(&c.format, "o", defFormat, "output format (shorthand)")
	fs.IntVar(&c.maxWidth, "max-width", 40, "max column width for table output")
	fs.IntVar(&c.maxRows, "max-rows", 0, "print at most N rows (0 = all)")
	fs.BoolVar(&c.noHeader, "no-header", false, "omit the header row")
	fs.BoolVar(&c.pretty, "pretty", false, "indent JSON output")

	fs.DurationVar(&c.timeout, "timeout", defaultTimeout, "statement timeout (0 = none)")
	fs.BoolVar(&c.readOnly, "read-only", envBool("BINSQL_READONLY"), "refuse any statement that would modify the database")
	fs.DurationVar(&c.secretTTL, "secret-ttl", envDuration("BINSQL_SECRET_TTL", secrets.DefaultTTL),
		"how long to cache a resolved vault secret (0 = never cache)")
}

// envDuration reads a duration from the environment, falling back to def.
func envDuration(key string, def time.Duration) time.Duration {
	raw := strings.TrimSpace(os.Getenv(key))
	if raw == "" {
		return def
	}
	d, err := time.ParseDuration(raw)
	if err != nil {
		return def
	}
	return d
}

func envBool(key string) bool {
	v := strings.ToLower(strings.TrimSpace(os.Getenv(key)))
	return v == "1" || v == "true" || v == "yes" || v == "on"
}

// renderOptions builds the output configuration.
func (c *commonFlags) renderOptions() (render.Options, error) {
	format, err := render.ParseFormat(c.format)
	if err != nil {
		return render.Options{}, usageError{err}
	}
	return render.Options{
		Format:   format,
		MaxWidth: c.maxWidth,
		MaxRows:  c.maxRows,
		NoHeader: c.noHeader,
		Pretty:   c.pretty,
	}, nil
}

// plan is a resolved connection target, known before anything is dialled.
// Keeping it separate lets the read-only guard reject a statement without
// opening a connection first.
type plan struct {
	driver   drivers.Driver
	name     string
	readOnly bool
	opts     render.Options
	timeout  time.Duration
	sqlOpts  sqlutil.Options

	// dsn may be a literal connection string or a vault reference such as
	// keyvault://my-vault/sql-connection-string. resolved holds the real
	// connection string once fetched; secrets are never written back to disk.
	dsn      string
	resolved string
	resolver *secrets.Resolver
}

// session is an open connection plus the settings it was opened with.
type session struct {
	*plan
	db      db.DB
	dialect db.Dialect
}

// lastFormat lets the error path render in the format the user asked for,
// even when the failure happened before a session existed.
var lastFormat = render.FormatTable

func renderErrorAs(w io.Writer, err error) {
	render.Error(w, err, render.Options{Format: lastFormat, Pretty: false})
}

// plan resolves which database to talk to, without connecting.
func (c *commonFlags) plan(ctx context.Context) (*plan, error) {
	opts, err := c.renderOptions()
	if err != nil {
		return nil, err
	}
	lastFormat = opts.Format

	driverName, dsn, readOnly, profile, err := c.resolveTarget()
	if err != nil {
		return nil, err
	}

	p := &plan{
		driver:   driverName,
		dsn:      dsn,
		name:     profile,
		readOnly: readOnly || c.readOnly,
		opts:     opts,
		timeout:  c.timeout,
		resolver: secrets.New(secrets.Options{Dir: config.Dir(), TTL: c.secretTTL}),
	}

	// A vault reference hides the driver, so when the profile does not record
	// one we have to fetch the secret before we can tell. Profiles saved by
	// `conn add` store the driver, which keeps the common path offline until
	// the connection is actually needed.
	if p.driver == "" {
		if _, err := p.connectionString(ctx); err != nil {
			return nil, err
		}
		p.driver, err = driverFromNameOrDSN("", p.resolved)
		if err != nil {
			return nil, err
		}
	}

	// Driver names double as dialect names for the lexer.
	p.sqlOpts = sqlutil.Options{Dialect: string(p.driver)}
	return p, nil
}

// connectionString returns the real DSN, fetching it from the vault the first
// time when the profile holds a reference.
func (p *plan) connectionString(ctx context.Context) (string, error) {
	if p.resolved != "" {
		return p.resolved, nil
	}
	if !secrets.IsRef(p.dsn) {
		p.resolved = p.dsn
		return p.resolved, nil
	}

	// The vault round-trip gets its own deadline: it is unrelated to how long
	// a statement may run, and --timeout 0 must not mean "hang forever here".
	vctx, cancel := context.WithTimeout(ctx, 30*time.Second)
	defer cancel()

	value, err := p.resolver.Resolve(vctx, p.dsn)
	if err != nil {
		return "", err
	}
	p.resolved = value
	return value, nil
}

// connect dials the planned target.
func (p *plan) connect(ctx context.Context) (*session, error) {
	dsn, err := p.connectionString(ctx)
	if err != nil {
		return nil, err
	}

	sdb, err := drivers.Open(p.driver, dsn)
	if err != nil {
		return nil, fmt.Errorf("connecting to %s (%s): %w", p.describe(), p.driver, err)
	}
	return &session{plan: p, db: sdb, dialect: sdb.Dialect()}, nil
}

// describe names the target for an error message without leaking a secret.
// For a vault reference the reference itself is the safest thing to print.
func (p *plan) describe() string {
	if secrets.IsRef(p.dsn) {
		return p.dsn
	}
	return config.MaskDSN(p.dsn)
}

// open resolves the connection and dials it, for commands that do not need to
// inspect the SQL before connecting.
func (c *commonFlags) open(ctx context.Context) (*session, error) {
	p, err := c.plan(ctx)
	if err != nil {
		return nil, err
	}
	return p.connect(ctx)
}

// resolveTarget works out which database to talk to, in precedence order:
// explicit --dsn, then --conn, then the configured default profile.
func (c *commonFlags) resolveTarget() (drivers.Driver, string, bool, string, error) {
	if c.dsn != "" {
		driver, err := c.driverFor(c.dsn)
		return driver, c.dsn, false, "", err
	}

	cfg, err := config.Load()
	if err != nil {
		return "", "", false, "", err
	}

	name := c.conn
	if name == "" {
		name = cfg.Default
	}
	if name == "" {
		return "", "", false, "", usagef(
			"no connection given: pass --dsn, or --conn NAME, or save a default with `binsql conn add <name> --dsn ...`")
	}

	profile, ok := cfg.Get(name)
	if !ok {
		known := cfg.Names()
		if len(known) == 0 {
			return "", "", false, "", usagef("no saved connection %q (none are configured; add one with `binsql conn add %s --dsn ...`)", name, name)
		}
		return "", "", false, "", usagef("no saved connection %q (known: %s)", name, strings.Join(known, ", "))
	}

	driverStr := profile.Driver
	if c.driver != "" {
		driverStr = c.driver
	}
	driver, err := driverFromNameOrDSN(driverStr, profile.DSN)
	if err != nil {
		return "", "", false, "", err
	}
	return driver, profile.DSN, profile.ReadOnly, name, nil
}

// driverFor resolves the driver for an explicit DSN.
func (c *commonFlags) driverFor(dsn string) (drivers.Driver, error) {
	return driverFromNameOrDSN(c.driver, dsn)
}

func driverFromNameOrDSN(name, dsn string) (drivers.Driver, error) {
	if name != "" {
		d, err := drivers.Parse(name)
		if err != nil {
			return "", usageError{err}
		}
		return d, nil
	}
	// A vault reference tells us nothing about the driver; the caller has to
	// resolve it first and try again with the real connection string.
	if secrets.IsRef(dsn) {
		return "", nil
	}
	if d := drivers.Infer(dsn); d != "" {
		return d, nil
	}
	return "", usagef("cannot infer the driver from %q — pass --driver (%s)",
		config.MaskDSN(dsn), strings.Join(drivers.Names(), ", "))
}

func (s *session) close() {
	if s != nil && s.db != nil {
		_ = s.db.Close()
	}
}

// context applies the statement timeout.
func (s *session) context(parent context.Context) (context.Context, context.CancelFunc) {
	if s.timeout <= 0 {
		return context.WithCancel(parent)
	}
	return context.WithTimeout(parent, s.timeout)
}

// status writes a human-facing note to stderr, keeping stdout clean for data.
func (p *plan) status(format string, args ...any) {
	fmt.Fprintf(os.Stderr, format+"\n", args...)
}

// guard enforces the read-only setting for a batch of statements. It runs
// before connecting, so a read-only violation is reported even when the
// database is unreachable.
func (p *plan) guard(statements []sqlutil.Statement) error {
	if !p.readOnly {
		return nil
	}
	for _, st := range statements {
		if st.Kind.Mutates() {
			where := "this connection is read-only"
			if p.name != "" {
				where = fmt.Sprintf("connection %q is read-only", p.name)
			}
			return fmt.Errorf("%s: refusing to run %s statement: %s",
				where, st.Kind, sqlutil.Summarize(st.SQL, p.sqlOpts, 60))
		}
	}
	return nil
}

// bind rewrites `?` placeholders into the driver's syntax and checks that the
// number of supplied arguments matches.
func (s *session) bind(sql string, args []any) (string, error) {
	out, n := sqlutil.Rewrite(sql, s.dialect.Placeholder, s.sqlOpts)
	if n != len(args) {
		// Postgres/SQL Server users may write native markers instead.
		if n == 0 && len(args) > 0 {
			return sql, nil
		}
		return "", usagef("statement has %d placeholder(s) but %d --arg value(s) were given", n, len(args))
	}
	return out, nil
}

// --- SQL input ------------------------------------------------------------

// readSQL collects the statement text from positional args, a file, or stdin.
func readSQL(args []string, file string) (string, error) {
	if file != "" {
		if file == "-" {
			data, err := io.ReadAll(os.Stdin)
			if err != nil {
				return "", fmt.Errorf("reading stdin: %w", err)
			}
			return string(data), nil
		}
		data, err := os.ReadFile(file)
		if err != nil {
			return "", fmt.Errorf("reading %s: %w", file, err)
		}
		return string(data), nil
	}

	if len(args) > 0 {
		joined := strings.TrimSpace(strings.Join(args, " "))
		if joined == "-" {
			data, err := io.ReadAll(os.Stdin)
			if err != nil {
				return "", fmt.Errorf("reading stdin: %w", err)
			}
			return string(data), nil
		}
		return joined, nil
	}

	// Nothing on the command line: accept a piped script.
	if !term.IsTerminal(int(os.Stdin.Fd())) {
		data, err := io.ReadAll(os.Stdin)
		if err != nil {
			return "", fmt.Errorf("reading stdin: %w", err)
		}
		if strings.TrimSpace(string(data)) != "" {
			return string(data), nil
		}
	}

	return "", usagef("no SQL given: pass it as an argument, with -f FILE, or on stdin")
}

// --- bind arguments -------------------------------------------------------

// argList collects repeated --arg flags.
type argList []any

func (a *argList) String() string { return "" }

func (a *argList) Set(raw string) error {
	v, err := parseArg(raw)
	if err != nil {
		return err
	}
	*a = append(*a, v)
	return nil
}

// parseArg converts a --arg value. Values are strings unless prefixed with a
// type: int:, float:, bool:, null:, json: or an explicit str:.
func parseArg(raw string) (any, error) {
	prefix, rest, found := strings.Cut(raw, ":")
	if !found {
		return raw, nil
	}

	switch strings.ToLower(prefix) {
	case "int":
		n, err := strconv.ParseInt(rest, 10, 64)
		if err != nil {
			return nil, fmt.Errorf("invalid int argument %q", rest)
		}
		return n, nil
	case "float":
		f, err := strconv.ParseFloat(rest, 64)
		if err != nil {
			return nil, fmt.Errorf("invalid float argument %q", rest)
		}
		return f, nil
	case "bool":
		b, err := strconv.ParseBool(rest)
		if err != nil {
			return nil, fmt.Errorf("invalid bool argument %q", rest)
		}
		return b, nil
	case "null":
		return nil, nil
	case "json":
		var v any
		if err := json.Unmarshal([]byte(rest), &v); err != nil {
			return nil, fmt.Errorf("invalid json argument: %w", err)
		}
		return rest, nil
	case "str", "string":
		return rest, nil
	default:
		// Not a recognised prefix — it was just a value containing a colon.
		return raw, nil
	}
}
