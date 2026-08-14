// Package cli implements binsql's non-interactive command mode: a set of
// subcommands for querying, modifying and inspecting a database from a
// script, a shell or an AI coding agent.
package cli

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"strings"
)

// ExitCode distinguishes usage mistakes from runtime failures.
const (
	exitOK    = 0
	exitError = 1
	exitUsage = 2
)

// usageError marks an error as the caller's fault, so main can exit 2.
type usageError struct{ err error }

func (u usageError) Error() string { return u.err.Error() }
func (u usageError) Unwrap() error { return u.err }

func usagef(format string, args ...any) error {
	return usageError{fmt.Errorf(format, args...)}
}

type command struct {
	name    string
	aliases []string
	summary string
	usage   string
	run     func(ctx context.Context, args []string) error
}

func commands() []*command {
	return []*command{
		queryCommand(),
		execCommand(),
		tablesCommand(),
		describeCommand(),
		schemaCommand(),
		countCommand(),
		headCommand(),
		connCommand(),
		tuiCommand(),
	}
}

func lookup(name string) *command {
	for _, c := range commands() {
		if c.name == name {
			return c
		}
		for _, a := range c.aliases {
			if a == name {
				return c
			}
		}
	}
	return nil
}

// IsCommand reports whether name is a known subcommand. main uses it to keep
// the original `binsql <driver> <dsn>` invocation working.
func IsCommand(name string) bool {
	switch name {
	case "help", "-h", "--help", "version", "--version":
		return true
	}
	return lookup(name) != nil
}

// Main runs a subcommand and returns a process exit code.
func Main(ctx context.Context, args []string) int {
	if len(args) == 0 {
		printUsage(os.Stderr)
		return exitUsage
	}

	name := args[0]
	switch name {
	case "help", "-h", "--help":
		if len(args) > 1 {
			if cmd := lookup(args[1]); cmd != nil {
				fmt.Fprintln(os.Stdout, strings.TrimSpace(cmd.usage))
				return exitOK
			}
		}
		printUsage(os.Stdout)
		return exitOK
	case "version", "--version", "-v":
		fmt.Println("binsql", Version)
		return exitOK
	}

	cmd := lookup(name)
	if cmd == nil {
		fmt.Fprintf(os.Stderr, "unknown command %q\n\n", name)
		printUsage(os.Stderr)
		return exitUsage
	}

	if err := cmd.run(ctx, args[1:]); err != nil {
		if errors.Is(err, flag.ErrHelp) {
			fmt.Fprintln(os.Stdout, strings.TrimSpace(cmd.usage))
			return exitOK
		}

		var ue usageError
		if errors.As(err, &ue) {
			fmt.Fprintln(os.Stderr, "error:", ue.Error())
			fmt.Fprintf(os.Stderr, "\nrun `binsql help %s` for usage\n", cmd.name)
			return exitUsage
		}

		reportError(err)
		return exitError
	}
	return exitOK
}

// Version is set from main so `binsql version` matches the build.
var Version = "dev"

// reportError writes an error in the shape the caller asked for, so a JSON
// consumer gets JSON on the error path too.
func reportError(err error) {
	renderErrorAs(os.Stderr, err)
}

func printUsage(w io.Writer) {
	fmt.Fprint(w, `binsql — SQL databases from the terminal

USAGE
  binsql <command> [flags] [arguments]      command mode (scripting, agents)
  binsql <driver> <dsn>                     interactive TUI
  binsql -q "<sql>" <driver> <dsn>          one-off query (legacy form)

COMMANDS
  query      Run a read-only query and print the result set
  exec       Run statements that modify data or schema
  tables     List tables and views
  describe   Show the columns of one table
  schema     Dump the schema of every table (or a subset)
  count      Count rows in a table
  head       Show the first rows of a table
  conn       Manage saved connection profiles
  tui        Launch the interactive terminal UI
  version    Print the version
  help       Show this help, or "binsql help <command>"

CONNECTION
  Every command accepts the same connection flags:

    -c, --conn NAME     use a saved profile (see "binsql conn")
    -d, --driver NAME   sqlite | postgres | mssql | mysql
    -D, --dsn STRING    connection string or file path

  The driver is inferred from the DSN when it is unambiguous, so
  "--dsn ./app.db" and "--dsn postgres://..." need no --driver.

  A DSN may also be an Azure Key Vault reference, read at connect time so
  no credential is stored on disk:

    binsql conn add prod --dsn "keyvault://my-vault/sql-connection-string"

  Environment: BINSQL_CONN, BINSQL_DRIVER, BINSQL_DSN, BINSQL_FORMAT,
  BINSQL_READONLY, BINSQL_SECRET_TTL, BINSQL_KEYVAULT_SUFFIX.

OUTPUT
  -o, --format FORMAT   table (default), json, jsonl, csv, tsv,
                        vertical, markdown, raw, none
      --max-rows N      print at most N rows (0 = all)
      --pretty          indent JSON output

EXAMPLES
  binsql conn add local --dsn ./app.db
  binsql query "select * from users where id = ?" --arg int:7 -o json
  binsql exec "update users set active = 0 where last_seen < ?" --arg 2024-01-01
  binsql exec -f migration.sql --dry-run
  binsql schema -o json
`)
}
