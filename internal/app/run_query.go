package app

import (
	"context"
	"os"

	"github.com/bgunnarsson/binsql/internal/db/sqlite"
	"github.com/bgunnarsson/binsql/internal/print"
)

func RunNonInteractive(ctx context.Context, path, query string) error {
	if query == "" {
		// default behaviour: list tables
		query = "select name from sqlite_master where type = 'table' order by name;"
	}

	sdb, err := sqlite.Open(path)
	if err != nil {
		return err
	}
	defer sdb.Close()

	rows, err := sdb.Query(ctx, query)
	if err != nil {
		return err
	}

	print.RenderTable(os.Stdout, rows, print.Options{MaxWidth: 60})
	return nil
}

