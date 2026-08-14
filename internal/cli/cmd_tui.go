package cli

import (
	"context"

	"github.com/bgunnarsson/binsql/internal/ui"
)

func tuiCommand() *command {
	return &command{
		name:    "tui",
		aliases: []string{"ui", "browse"},
		summary: "Launch the interactive terminal UI",
		usage: `binsql tui — launch the interactive terminal UI

USAGE
  binsql tui [flags]

Opens the full-screen browser against a saved profile or an explicit DSN.
Equivalent to the original ` + "`binsql <driver> <dsn>`" + ` form, but able to
use ` + "`--conn`" + `.

FLAGS
  the shared connection flags — see ` + "`binsql help`" + `.

EXAMPLES
  binsql tui --conn local
  binsql tui --dsn ./app.db
`,
		run: runTUI,
	}
}

func runTUI(ctx context.Context, argv []string) error {
	var common commonFlags

	fs := newFlagSet("tui", "")
	common.register(fs, 0)
	if err := parseFlags(fs, argv); err != nil {
		return usageError{err}
	}
	if fs.NArg() > 0 {
		return usagef("tui takes no positional arguments (pass --conn or --dsn)")
	}

	sess, err := common.open(ctx)
	if err != nil {
		return err
	}
	defer sess.close()

	return ui.Run(ctx, sess.db, string(sess.driver))
}
