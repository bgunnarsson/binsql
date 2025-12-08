// cmd/binsql/main.go
package main

import (
	"context"
	"flag"
	"fmt"
	"os"

	"golang.org/x/term"

	"github.com/bgunnarsson/binsql/internal/app"
)

func main() {
	// Workaround: azidentity/AzureCLICredential treats any stderr as error.
	// Azure CLI on macOS+Py3.12 spews SyntaxWarning to stderr. Kill them.
	if os.Getenv("PYTHONWARNINGS") == "" {
		// ignore all Python warnings inside az
		os.Setenv("PYTHONWARNINGS", "ignore")
	}

	var query string
	flag.StringVar(&query, "q", "", "SQL query to run in non-interactive mode")
	flag.Parse()

	if flag.NArg() < 2 {
		fmt.Fprintln(os.Stderr, "usage: binsql [flags] <sqlite|postgres|mssql|mysql> <database-path-or-dsn>")
		flag.PrintDefaults()
		os.Exit(2)
	}

	driverStr := flag.Arg(0)
	dsn := flag.Arg(1)

	var driver app.Driver
	switch driverStr {
	case string(app.DriverSqlite):
		driver = app.DriverSqlite
	case string(app.DriverPostgres):
		driver = app.DriverPostgres
	case string(app.DriverMssql):
		driver = app.DriverMssql
	case string(app.DriverMysql):
		driver = app.DriverMysql	
	default:
		fmt.Fprintf(os.Stderr, "unknown driver %q (expected sqlite, postgres, mssql or mysql)\n", driverStr)
		os.Exit(2)
	}

	ctx := context.Background()
	stdoutIsTTY := term.IsTerminal(int(os.Stdout.Fd()))

	if query != "" || !stdoutIsTTY {
		if err := app.RunNonInteractive(ctx, driver, dsn, query); err != nil {
			fmt.Fprintln(os.Stderr, "error:", err)
			os.Exit(1)
		}
		return
	}

	if err := app.RunInteractive(ctx, driver, dsn); err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		os.Exit(1)
	}
}
