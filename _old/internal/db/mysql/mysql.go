package mysql

import (
	"context"
	"database/sql"
	"fmt"
	"strings"
	"time"

	_ "github.com/go-sql-driver/mysql"

	"github.com/bgunnarsson/binsql/internal/db"
	"github.com/bgunnarsson/binsql/internal/db/sqlcore"
)

type MysqlDB struct {
	*sqlcore.Core
}

func Open(dsn string) (*MysqlDB, error) {
	if dsn == "" {
		return nil, fmt.Errorf("empty mysql DSN")
	}

	sqldb, err := sql.Open("mysql", dsn)
	if err != nil {
		return nil, err
	}

	sqldb.SetMaxOpenConns(4)
	sqldb.SetMaxIdleConns(4)
	sqldb.SetConnMaxLifetime(5 * time.Minute)

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	if err := sqldb.PingContext(ctx); err != nil {
		sqldb.Close()
		return nil, err
	}

	return &MysqlDB{Core: &sqlcore.Core{SQL: sqldb, Conv: convert}}, nil
}

func convert(v any, dbType string) any {
	switch x := v.(type) {
	case []byte:
		// MySQL returns TEXT/VARCHAR as []byte; real binary columns stay raw
		// so the renderers can flag them instead of emitting mojibake.
		switch dbType {
		case "binary", "varbinary", "blob", "tinyblob", "mediumblob", "longblob", "geometry":
			return x
		default:
			return string(x)
		}
	case time.Time:
		return x.Format(time.RFC3339Nano)
	default:
		return x
	}
}

func (m *MysqlDB) Dialect() db.Dialect {
	return db.Dialect{
		Name:        "mysql",
		Placeholder: func(int) string { return "?" },
		QuoteIdent:  quoteIdent,
		SelectLimit: selectLimit,
	}
}

func (m *MysqlDB) ListTables(ctx context.Context) ([]string, error) {
	const q = `
SELECT table_name
FROM information_schema.tables
WHERE table_type IN ('BASE TABLE', 'VIEW')
  AND table_schema = DATABASE()
ORDER BY table_name;
`
	rows, err := m.SQL.QueryContext(ctx, q)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []string
	for rows.Next() {
		var name string
		if err := rows.Scan(&name); err != nil {
			return nil, err
		}
		out = append(out, name)
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	return out, nil
}

func (m *MysqlDB) DescribeTable(ctx context.Context, table string) ([]db.Column, error) {
	// Strip an explicit database qualifier; the DSN already selects one.
	if dot := strings.Index(table, "."); dot != -1 {
		table = table[dot+1:]
	}

	const q = `
SELECT column_name, column_type, is_nullable,
       COALESCE(column_default, '') AS col_default,
       column_key
FROM information_schema.columns
WHERE table_schema = DATABASE()
  AND table_name = ?
ORDER BY ordinal_position;
`
	rows, err := m.SQL.QueryContext(ctx, q, table)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var cols []db.Column
	for rows.Next() {
		var colName, dataType, isNullable, colDefault, colKey string
		if err := rows.Scan(&colName, &dataType, &isNullable, &colDefault, &colKey); err != nil {
			return nil, err
		}
		cols = append(cols, db.Column{
			Name:       colName,
			Type:       dataType,
			Nullable:   strings.EqualFold(isNullable, "YES"),
			Default:    colDefault,
			PrimaryKey: colKey == "PRI",
		})
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	if len(cols) == 0 {
		return nil, fmt.Errorf("table %q not found", table)
	}
	return cols, nil
}

func selectLimit(table, where string, n int) string {
	q := "SELECT * FROM " + table
	if where != "" {
		q += " WHERE " + where
	}
	if n > 0 {
		q += fmt.Sprintf(" LIMIT %d", n)
	}
	return q
}

func quoteIdent(id string) string {
	return "`" + strings.ReplaceAll(id, "`", "``") + "`"
}
