package app

import (
	"context"

	"github.com/bgunnarsson/binsql/internal/db"
	"github.com/bgunnarsson/binsql/internal/drivers"
	"github.com/bgunnarsson/binsql/internal/ui"
)

// Driver and the Driver* constants are re-exported from internal/drivers so
// existing callers keep compiling.
type Driver = drivers.Driver

const (
	DriverSqlite   = drivers.DriverSqlite
	DriverPostgres = drivers.DriverPostgres
	DriverMssql    = drivers.DriverMssql
	DriverMysql    = drivers.DriverMysql
)

func openDB(driver Driver, dsn string) (db.DB, error) {
	return drivers.Open(driver, dsn)
}

func RunInteractive(ctx context.Context, driver Driver, dsn string) error {
	sdb, err := openDB(driver, dsn)
	if err != nil {
		return err
	}
	defer sdb.Close()

	// Label for prompt/header
	label := "sqlite"
	if driver != "" {
		label = string(driver)
	}

	return ui.Run(ctx, sdb, label)
}
