package app

import (
	"context"
	"fmt"

	"github.com/bgunnarsson/binsql/internal/db"
	"github.com/bgunnarsson/binsql/internal/db/mssql"
	"github.com/bgunnarsson/binsql/internal/db/mysql"
	"github.com/bgunnarsson/binsql/internal/db/postgres"
	"github.com/bgunnarsson/binsql/internal/db/sqlite"
	"github.com/bgunnarsson/binsql/internal/ui"
)

type Driver string

const (
	DriverSqlite   Driver = "sqlite"
	DriverPostgres Driver = "postgres"
	DriverMssql    Driver = "mssql"
	DriverMysql    Driver = "mysql"
)

// central factory
func openDB(driver Driver, dsn string) (db.DB, error) {
	switch driver {
	case "", DriverSqlite:
		return sqlite.Open(dsn)
	case DriverPostgres:
		return postgres.Open(dsn)
	case DriverMssql:
		return mssql.Open(dsn)
	case DriverMysql:
		return mysql.Open(dsn)
	default:
		return nil, fmt.Errorf("unsupported driver %q", driver)
	}
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
