// Package drivers maps driver names and DSNs onto concrete db.DB adapters.
package drivers

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/bgunnarsson/binsql/internal/db"
	"github.com/bgunnarsson/binsql/internal/db/mssql"
	"github.com/bgunnarsson/binsql/internal/db/mysql"
	"github.com/bgunnarsson/binsql/internal/db/postgres"
	"github.com/bgunnarsson/binsql/internal/db/sqlite"
)

type Driver string

const (
	DriverSqlite   Driver = "sqlite"
	DriverPostgres Driver = "postgres"
	DriverMssql    Driver = "mssql"
	DriverMysql    Driver = "mysql"
)

// All returns every supported driver name, in documentation order.
func All() []Driver {
	return []Driver{DriverSqlite, DriverPostgres, DriverMssql, DriverMysql}
}

// Names returns All() as strings, for help text and error messages.
func Names() []string {
	out := make([]string, 0, 4)
	for _, d := range All() {
		out = append(out, string(d))
	}
	return out
}

// Parse validates a driver name, accepting a few common aliases.
func Parse(s string) (Driver, error) {
	switch strings.ToLower(strings.TrimSpace(s)) {
	case "sqlite", "sqlite3":
		return DriverSqlite, nil
	case "postgres", "postgresql", "pg", "pgx":
		return DriverPostgres, nil
	case "mssql", "sqlserver", "azuresql":
		return DriverMssql, nil
	case "mysql", "mariadb":
		return DriverMysql, nil
	default:
		return "", fmt.Errorf("unknown driver %q (expected one of: %s)", s, strings.Join(Names(), ", "))
	}
}

// IsName reports whether s names a driver. Used to tell the legacy
// `binsql <driver> <dsn>` form apart from the subcommand form.
func IsName(s string) bool {
	_, err := Parse(s)
	return err == nil
}

var mysqlDSN = regexp.MustCompile(`^[^:/@]+(:[^@]*)?@(tcp|unix)\(`)

// Infer guesses the driver from the shape of a DSN. It is deliberately
// conservative: an empty result means "could not tell, ask the user".
func Infer(dsn string) Driver {
	trimmed := strings.TrimSpace(dsn)
	lower := strings.ToLower(trimmed)

	switch {
	case strings.HasPrefix(lower, "postgres://"), strings.HasPrefix(lower, "postgresql://"):
		return DriverPostgres
	case strings.HasPrefix(lower, "sqlserver://"), strings.HasPrefix(lower, "azuresql://"):
		return DriverMssql
	case strings.HasPrefix(lower, "mysql://"):
		return DriverMysql
	case strings.HasPrefix(lower, "sqlite://"), strings.HasPrefix(lower, "file:"):
		return DriverSqlite
	}

	// Key=value connection strings are SQL Server's native form.
	if strings.Contains(lower, "fedauth=") ||
		(strings.Contains(lower, "server=") && strings.Contains(lower, ";")) {
		return DriverMssql
	}

	// go-sql-driver/mysql: user:pass@tcp(host:port)/db
	if mysqlDSN.MatchString(trimmed) {
		return DriverMysql
	}

	// libpq keyword form: host=... dbname=...
	if strings.Contains(lower, "dbname=") || strings.Contains(lower, "sslmode=") {
		return DriverPostgres
	}

	// A path that looks like or resolves to a file on disk.
	switch strings.ToLower(filepath.Ext(trimmed)) {
	case ".db", ".sqlite", ".sqlite3", ".db3":
		return DriverSqlite
	}
	if trimmed != "" && !strings.Contains(trimmed, "://") {
		if st, err := os.Stat(trimmed); err == nil && !st.IsDir() {
			return DriverSqlite
		}
	}

	return ""
}

// Open builds the adapter for a driver. An empty driver defaults to sqlite,
// preserving the original behaviour.
func Open(driver Driver, dsn string) (db.DB, error) {
	switch driver {
	case "", DriverSqlite:
		return sqlite.Open(stripScheme(dsn, "sqlite://"))
	case DriverPostgres:
		return postgres.Open(dsn)
	case DriverMssql:
		return mssql.Open(dsn)
	case DriverMysql:
		return mysql.Open(stripScheme(dsn, "mysql://"))
	default:
		return nil, fmt.Errorf("unsupported driver %q", driver)
	}
}

// stripScheme removes a scheme prefix the underlying driver does not accept.
func stripScheme(dsn, scheme string) string {
	if strings.HasPrefix(strings.ToLower(dsn), scheme) {
		return dsn[len(scheme):]
	}
	return dsn
}
