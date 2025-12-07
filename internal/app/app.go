package app

import (
	"context"

	"github.com/bgunnarsson/binsql/internal/db/sqlite"
	"github.com/bgunnarsson/binsql/internal/ui"
)

func RunInteractive(ctx context.Context, path string) error {
	sdb, err := sqlite.Open(path)
	if err != nil {
		return err
	}
	defer sdb.Close()

	return ui.Run(ctx, sdb)
}
