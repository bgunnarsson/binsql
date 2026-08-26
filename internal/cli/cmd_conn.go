package cli

import (
	"context"
	"flag"
	"fmt"
	"os"
	"time"

	"github.com/bgunnarsson/binsql/internal/config"
	"github.com/bgunnarsson/binsql/internal/db"
	"github.com/bgunnarsson/binsql/internal/render"
	"github.com/bgunnarsson/binsql/internal/secrets"
)

func connCommand() *command {
	return &command{
		name:    "conn",
		aliases: []string{"conns", "connections"},
		summary: "Manage saved connection profiles",
		usage: `binsql conn — manage saved connection profiles

USAGE
  binsql conn list
  binsql conn add <name> --dsn <dsn> [--driver X] [--readonly] [--description T]
  binsql conn remove <name>
  binsql conn default <name>
  binsql conn test [name]
  binsql conn show <name> [--show-secrets]
  binsql conn cache [--clear]

Profiles live in ` + "`~/.config/binsql/connections.json`" + ` (override with
BINSQL_CONFIG) and are written with owner-only permissions, since a DSN
usually contains a password. Passwords are masked when printed.

Once a profile exists, every command accepts ` + "`--conn <name>`" + `; the
profile marked default is used when no connection is given at all.

Mark production databases with --readonly and binsql will refuse any
statement that would modify them, no matter how it is invoked.

AZURE KEY VAULT
  A profile can reference a secret instead of storing a connection string,
  so no credential is ever written to disk:

    binsql conn add prod --dsn "keyvault://my-vault/sql-connection-string"

  The secret is read at connect time using DefaultAzureCredential, which
  covers "az login" locally, a managed identity on Azure, and
  AZURE_CLIENT_ID/AZURE_TENANT_ID/AZURE_CLIENT_SECRET in CI. Adding the
  profile reads the secret once to verify access and record the driver;
  pass --no-verify (with --driver) to skip that.

  Resolved secrets are cached encrypted under the config directory for
  --secret-ttl (default 15m). See "binsql conn cache".

EXAMPLES
  binsql conn add local --dsn ./app.db
  binsql conn add prod --dsn "postgres://u:p@host/db" --readonly
  binsql conn add prod --dsn "keyvault://my-vault/prod-conn" --readonly
  binsql conn default local
  binsql conn test prod
  binsql conn cache --clear
`,
		run: runConn,
	}
}

func runConn(ctx context.Context, argv []string) error {
	if len(argv) == 0 {
		return runConnList(ctx, nil)
	}

	sub := argv[0]
	rest := argv[1:]

	switch sub {
	case "list", "ls":
		return runConnList(ctx, rest)
	case "add", "set":
		return runConnAdd(ctx, rest)
	case "remove", "rm", "delete":
		return runConnRemove(ctx, rest)
	case "default":
		return runConnDefault(ctx, rest)
	case "test", "ping":
		return runConnTest(ctx, rest)
	case "show":
		return runConnShow(ctx, rest)
	case "cache":
		return runConnCache(ctx, rest)
	default:
		return usagef("unknown `conn` subcommand %q (expected list, add, remove, default, test, show or cache)", sub)
	}
}

// outputFlags is the subset of commonFlags that applies to commands which do
// not open a database connection.
type outputFlags struct {
	format string
	pretty bool
}

func (o *outputFlags) register(fs *flag.FlagSet) {
	fs.StringVar(&o.format, "format", os.Getenv("BINSQL_FORMAT"), "output format")
	fs.StringVar(&o.format, "o", os.Getenv("BINSQL_FORMAT"), "output format (shorthand)")
	fs.BoolVar(&o.pretty, "pretty", false, "indent JSON output")
}

func (o *outputFlags) options() (render.Options, error) {
	format, err := render.ParseFormat(o.format)
	if err != nil {
		return render.Options{}, usageError{err}
	}
	lastFormat = format
	return render.Options{Format: format, Pretty: o.pretty, MaxWidth: 60}, nil
}

func runConnList(_ context.Context, argv []string) error {
	var out outputFlags
	fs := newFlagSet("conn list", "")
	out.register(fs)
	showSecrets := fs.Bool("show-secrets", false, "print DSNs unmasked")
	if err := parseFlags(fs, argv); err != nil {
		return usageError{err}
	}
	opts, err := out.options()
	if err != nil {
		return err
	}

	cfg, err := config.Load()
	if err != nil {
		return err
	}

	names := cfg.Names()
	if len(names) == 0 && !opts.Format.Structured() {
		fmt.Fprintf(os.Stderr, "no saved connections yet — add one:\n  binsql conn add local --dsn ./app.db\n")
		fmt.Fprintf(os.Stderr, "config file: %s\n", config.Path())
		return nil
	}

	rows := &db.Rows{Columns: []db.Column{
		{Name: "name"}, {Name: "driver"}, {Name: "dsn"}, {Name: "readonly"}, {Name: "default"},
	}}
	for _, name := range names {
		conn := cfg.Connections[name]
		dsn := conn.DSN
		if !*showSecrets {
			dsn = config.MaskDSN(dsn)
		}
		rows.Data = append(rows.Data, db.Row{
			name, conn.Driver, dsn, yesNo(conn.ReadOnly), yesNo(cfg.Default == name),
		})
	}

	return render.Rows(os.Stdout, render.Result{Rows: rows}, opts)
}

func runConnAdd(ctx context.Context, argv []string) error {
	var out outputFlags
	fs := newFlagSet("conn add", "")
	out.register(fs)
	dsn := fs.String("dsn", "", "connection string, file path or keyvault:// reference")
	fs.StringVar(dsn, "D", "", "connection string (shorthand)")
	driverName := fs.String("driver", "", "sqlite|postgres|mssql|mysql")
	fs.StringVar(driverName, "d", "", "driver (shorthand)")
	description := fs.String("description", "", "human-readable note")
	readOnly := fs.Bool("readonly", false, "refuse all mutating statements on this connection")
	makeDefault := fs.Bool("default", false, "also make this the default connection")
	noVerify := fs.Bool("no-verify", false, "do not contact the vault to verify a keyvault:// reference")
	if err := parseFlags(fs, argv); err != nil {
		return usageError{err}
	}
	if _, err := out.options(); err != nil {
		return err
	}

	if fs.NArg() != 1 {
		return usagef("conn add takes exactly one name")
	}
	name := fs.Arg(0)
	if *dsn == "" {
		return usagef("--dsn is required")
	}

	// Check this before resolving the driver: passing --driver would
	// otherwise skip inference altogether and save a vault URL as if it were
	// a literal connection string, failing much later with a worse message.
	if err := checkVaultOnly(*dsn); err != nil {
		return err
	}

	driver, err := driverFromNameOrDSN(*driverName, *dsn)
	if err != nil {
		return err
	}

	// Fetch a vault reference once, now: it confirms the secret exists and
	// that we can read it, and it reveals the driver so that later commands
	// need no vault round-trip before deciding how to parse SQL.
	if secrets.IsRef(*dsn) {
		if *noVerify {
			if driver == "" {
				return usagef("--no-verify skips reading the secret, so --driver is required")
			}
		} else {
			resolved, err := resolveSecret(ctx, *dsn)
			if err != nil {
				return err
			}
			if driver == "" {
				if driver, err = driverFromNameOrDSN("", resolved); err != nil {
					return fmt.Errorf("the secret was read, but %w", err)
				}
			}
			fmt.Fprintf(os.Stderr, "verified %s (resolves to a %s connection string)\n", *dsn, driver)
		}
	}

	cfg, err := config.Load()
	if err != nil {
		return err
	}
	_, existed := cfg.Get(name)
	cfg.Set(name, config.Connection{
		Driver:      string(driver),
		DSN:         *dsn,
		Description: *description,
		ReadOnly:    *readOnly,
	})
	if *makeDefault || cfg.Default == "" {
		cfg.Default = name
	}
	if err := cfg.Save(); err != nil {
		return err
	}

	verb := "saved"
	if existed {
		verb = "updated"
	}
	fmt.Fprintf(os.Stderr, "%s connection %q (%s → %s)\n", verb, name, driver, config.MaskDSN(*dsn))
	if cfg.Default == name {
		fmt.Fprintf(os.Stderr, "%q is now the default connection\n", name)
	}
	return nil
}

func runConnRemove(_ context.Context, argv []string) error {
	fs := newFlagSet("conn remove", "")
	if err := parseFlags(fs, argv); err != nil {
		return usageError{err}
	}
	if fs.NArg() != 1 {
		return usagef("conn remove takes exactly one name")
	}
	name := fs.Arg(0)

	cfg, err := config.Load()
	if err != nil {
		return err
	}
	if !cfg.Remove(name) {
		return fmt.Errorf("no saved connection %q", name)
	}
	if err := cfg.Save(); err != nil {
		return err
	}
	fmt.Fprintf(os.Stderr, "removed connection %q\n", name)
	return nil
}

func runConnDefault(_ context.Context, argv []string) error {
	fs := newFlagSet("conn default", "")
	if err := parseFlags(fs, argv); err != nil {
		return usageError{err}
	}

	cfg, err := config.Load()
	if err != nil {
		return err
	}

	if fs.NArg() == 0 {
		if cfg.Default == "" {
			fmt.Fprintln(os.Stderr, "no default connection set")
			return nil
		}
		fmt.Println(cfg.Default)
		return nil
	}
	if fs.NArg() != 1 {
		return usagef("conn default takes at most one name")
	}

	name := fs.Arg(0)
	if _, ok := cfg.Get(name); !ok {
		return fmt.Errorf("no saved connection %q", name)
	}
	cfg.Default = name
	if err := cfg.Save(); err != nil {
		return err
	}
	fmt.Fprintf(os.Stderr, "default connection is now %q\n", name)
	return nil
}

func runConnShow(_ context.Context, argv []string) error {
	var out outputFlags
	fs := newFlagSet("conn show", "")
	out.register(fs)
	showSecrets := fs.Bool("show-secrets", false, "print the DSN unmasked")
	if err := parseFlags(fs, argv); err != nil {
		return usageError{err}
	}
	opts, err := out.options()
	if err != nil {
		return err
	}
	if fs.NArg() != 1 {
		return usagef("conn show takes exactly one name")
	}
	name := fs.Arg(0)

	cfg, err := config.Load()
	if err != nil {
		return err
	}
	conn, ok := cfg.Get(name)
	if !ok {
		return fmt.Errorf("no saved connection %q", name)
	}

	dsn := conn.DSN
	if !*showSecrets {
		dsn = config.MaskDSN(dsn)
	}

	rows := &db.Rows{Columns: []db.Column{{Name: "field"}, {Name: "value"}}}
	rows.Data = append(rows.Data,
		db.Row{"name", name},
		db.Row{"driver", conn.Driver},
		db.Row{"dsn", dsn},
		db.Row{"readonly", yesNo(conn.ReadOnly)},
		db.Row{"default", yesNo(cfg.Default == name)},
		db.Row{"description", conn.Description},
	)
	return render.Rows(os.Stdout, render.Result{Rows: rows}, opts)
}

func runConnTest(ctx context.Context, argv []string) error {
	var common commonFlags
	fs := newFlagSet("conn test", "")
	common.register(fs, 15*time.Second)
	if err := parseFlags(fs, argv); err != nil {
		return usageError{err}
	}
	if fs.NArg() == 1 && common.conn == "" {
		common.conn = fs.Arg(0)
	} else if fs.NArg() > 1 {
		return usagef("conn test takes at most one name")
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
		return fmt.Errorf("connected, but listing tables failed: %w", err)
	}

	label := string(sess.driver)
	if sess.name != "" {
		label = fmt.Sprintf("%s (%s)", sess.name, sess.driver)
	}
	fmt.Fprintf(os.Stderr, "ok: %s — %d table(s)\n", label, len(tables))
	if sess.readOnly {
		fmt.Fprintln(os.Stderr, "note: this connection is read-only")
	}
	return nil
}

func yesNo(b bool) string {
	if b {
		return "yes"
	}
	return "no"
}

// resolveSecret reads a vault reference once, for verification paths that do
// not build a full plan.
func resolveSecret(ctx context.Context, ref string) (string, error) {
	resolver := secrets.New(secrets.Options{
		Dir: config.Dir(),
		TTL: envDuration("BINSQL_SECRET_TTL", secrets.DefaultTTL),
	})

	vctx, cancel := context.WithTimeout(ctx, 30*time.Second)
	defer cancel()

	return resolver.Resolve(vctx, ref)
}

func runConnCache(_ context.Context, argv []string) error {
	fs := newFlagSet("conn cache", "")
	doClear := fs.Bool("clear", false, "delete every cached secret and the local key")
	if err := parseFlags(fs, argv); err != nil {
		return usageError{err}
	}

	cache := &secrets.Cache{
		Dir: config.Dir(),
		TTL: envDuration("BINSQL_SECRET_TTL", secrets.DefaultTTL),
	}

	if *doClear {
		if err := cache.Clear(); err != nil {
			return err
		}
		fmt.Fprintln(os.Stderr, "cleared the secret cache")
		return nil
	}

	fmt.Fprintf(os.Stderr, "cached secrets: %d\n", cache.Count())
	fmt.Fprintf(os.Stderr, "location:       %s\n", config.Dir())
	fmt.Fprintf(os.Stderr, "ttl:            %s\n", cache.TTL)
	return nil
}
