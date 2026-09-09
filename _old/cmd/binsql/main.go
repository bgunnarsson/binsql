// cmd/binsql/main.go
package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"strings"

	"golang.org/x/term"

	"github.com/bgunnarsson/binsql/internal/app"
	"github.com/bgunnarsson/binsql/internal/cli"
	"github.com/bgunnarsson/binsql/internal/drivers"
)

// version is overridable at build time:
//
//	go build -ldflags="-X main.version=2.1.0" ./cmd/binsql
var version = "dev"

func main() {
	// Workaround: azidentity/AzureCLICredential treats any stderr as error.
	// Azure CLI on macOS+Py3.12 spews SyntaxWarning to stderr. Kill them.
	if os.Getenv("PYTHONWARNINGS") == "" {
		// ignore all Python warnings inside az
		os.Setenv("PYTHONWARNINGS", "ignore")
	}

	cli.Version = version
	ctx := context.Background()
	args := os.Args[1:]

	// Two invocation styles share one binary:
	//
	//   binsql <driver> <dsn>      the original positional form
	//   binsql <command> [flags]   command mode
	//
	// They are told apart by the first positional argument: a driver name
	// keeps the old behaviour, anything else is dispatched as a command.
	if !legacyInvocation(args) {
		os.Exit(cli.Main(ctx, args))
	}

	os.Exit(runLegacy(ctx, args))
}

// legacyInvocation reports whether the first positional argument names a
// driver, which is the shape the original CLI accepted.
func legacyInvocation(args []string) bool {
	for i := 0; i < len(args); i++ {
		a := args[i]
		// -q takes a value, so skip what follows it.
		if a == "-q" || a == "--q" {
			i++
			continue
		}
		if strings.HasPrefix(a, "-") {
			continue
		}
		return drivers.IsName(a)
	}
	return false
}

// runLegacy preserves `binsql [-q sql] <driver> <dsn>` exactly as it behaved
// before command mode existed.
func runLegacy(ctx context.Context, args []string) int {
	fs := flag.NewFlagSet("binsql", flag.ExitOnError)
	query := fs.String("q", "", "SQL query to run in non-interactive mode")
	if err := fs.Parse(args); err != nil {
		return 2
	}

	if fs.NArg() < 2 {
		fmt.Fprintln(os.Stderr, "usage: binsql [flags] <sqlite|postgres|mssql|mysql> <database-path-or-dsn>")
		fmt.Fprintln(os.Stderr, "       binsql <command> [flags]   (run `binsql help` for command mode)")
		fs.PrintDefaults()
		return 2
	}

	driver, err := drivers.Parse(fs.Arg(0))
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 2
	}
	dsn := fs.Arg(1)

	stdoutIsTTY := term.IsTerminal(int(os.Stdout.Fd()))

	if *query != "" || !stdoutIsTTY {
		if err := app.RunNonInteractive(ctx, driver, dsn, *query); err != nil {
			fmt.Fprintln(os.Stderr, "error:", err)
			return 1
		}
		return 0
	}

	if err := app.RunInteractive(ctx, driver, dsn); err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		return 1
	}
	return 0
}
