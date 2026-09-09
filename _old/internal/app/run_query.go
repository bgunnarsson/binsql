package app

import (
	"context"
	"os"

	"github.com/bgunnarsson/binsql/internal/render"
)

// defaultListQuery returns the driver-specific "list tables" SQL used
// when the user does not provide a query in non-interactive mode.
func defaultListQuery(driver Driver) string {
	switch driver {
	case DriverPostgres:
		return `
SELECT table_schema || '.' || table_name AS name
FROM information_schema.tables
WHERE table_type = 'BASE TABLE'
  AND table_schema NOT IN ('pg_catalog', 'information_schema')
ORDER BY table_schema, table_name;
`
	case DriverMssql:
		return `
SELECT TABLE_SCHEMA + '.' + TABLE_NAME AS name
FROM INFORMATION_SCHEMA.TABLES
WHERE TABLE_TYPE = 'BASE TABLE'
ORDER BY TABLE_SCHEMA, TABLE_NAME;
`
	case DriverMysql:
		return `
SELECT table_name AS name
FROM information_schema.tables
WHERE table_type = 'BASE TABLE'
  AND table_schema = DATABASE()
ORDER BY table_name;
`
	case "", DriverSqlite:
		fallthrough
	default:
		return "select name from sqlite_master where type = 'table' order by name;"
	}
}

func RunNonInteractive(ctx context.Context, driver Driver, dsn, query string) error {
	if query == "" {
		query = defaultListQuery(driver)
	}

	sdb, err := openDB(driver, dsn)
	if err != nil {
		return err
	}
	defer sdb.Close()

	rows, err := sdb.Query(ctx, query)
	if err != nil {
		return err
	}

	// NoFooter keeps this legacy path's output identical to what it printed
	// before command mode existed.
	return render.Rows(os.Stdout, render.Result{Rows: rows}, render.Options{
		Format:   render.FormatTable,
		MaxWidth: 60,
		NoFooter: true,
	})
}
