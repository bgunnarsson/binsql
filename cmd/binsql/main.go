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
	var query string

	flag.StringVar(&query, "q", "", "SQL query to run in non-interactive mode")
	flag.Parse()

	if flag.NArg() < 1 {
		fmt.Fprintln(os.Stderr, "usage: binsql [-q query] <db.sqlite>")
		os.Exit(1)
	}

	dbPath := flag.Arg(0)

	ctx := context.Background()

	stdoutIsTTY := term.IsTerminal(int(os.Stdout.Fd()))
	if query != "" || !stdoutIsTTY {
		if err := app.RunNonInteractive(ctx, dbPath, query); err != nil {
			fmt.Fprintln(os.Stderr, "error:", err)
			os.Exit(1)
		}
		return
	}

	if err := app.RunInteractive(ctx, dbPath); err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		os.Exit(1)
	}
}

