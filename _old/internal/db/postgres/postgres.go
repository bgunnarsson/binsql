package postgres

import (
	"context"
	"database/sql"
	"fmt"
	"strconv"
	"strings"
	"time"

	_ "github.com/jackc/pgx/v5/stdlib" // register pgx stdlib driver

	"github.com/bgunnarsson/binsql/internal/db"
	"github.com/bgunnarsson/binsql/internal/db/sqlcore"
)

type PostgresDB struct {
	*sqlcore.Core
}

func Open(dsn string) (*PostgresDB, error) {
	if dsn == "" {
		return nil, fmt.Errorf("empty postgres DSN")
	}

	sqldb, err := sql.Open("pgx", dsn)
	if err != nil {
		return nil, err
	}

	// Sane defaults for a small CLI tool.
	sqldb.SetMaxOpenConns(4)
	sqldb.SetMaxIdleConns(4)
	sqldb.SetConnMaxLifetime(5 * time.Minute)

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	if err := sqldb.PingContext(ctx); err != nil {
		sqldb.Close()
		return nil, err
	}

	return &PostgresDB{Core: &sqlcore.Core{SQL: sqldb, Conv: convert}}, nil
}

func convert(v any, _ string) any {
	switch x := v.(type) {
	case []byte:
		return string(x)
	case time.Time:
		return x.Format(time.RFC3339Nano)
	default:
		return x
	}
}

func (p *PostgresDB) Dialect() db.Dialect {
	return db.Dialect{
		Name:        "postgres",
		Placeholder: func(n int) string { return "$" + strconv.Itoa(n) },
		QuoteIdent:  quoteIdent,
		SelectLimit: selectLimit,
	}
}

func (p *PostgresDB) ListTables(ctx context.Context) ([]string, error) {
	const q = `
SELECT table_schema || '.' || table_name AS name
FROM information_schema.tables
WHERE table_type IN ('BASE TABLE', 'VIEW')
  AND table_schema NOT IN ('pg_catalog', 'information_schema')
ORDER BY table_schema, table_name;
`
	rows, err := p.SQL.QueryContext(ctx, q)
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

// DescribeTable returns column name, type, nullability, default and primary
// key membership. Accepts either "table" or "schema.table".
func (p *PostgresDB) DescribeTable(ctx context.Context, table string) ([]db.Column, error) {
	schema := "public"
	name := table
	if dot := strings.Index(table, "."); dot != -1 {
		schema = table[:dot]
		name = table[dot+1:]
	}

	const q = `
SELECT
  c.column_name,
  c.data_type || CASE
    WHEN c.character_maximum_length IS NOT NULL
      THEN '(' || c.character_maximum_length || ')'
    ELSE ''
  END AS type,
  c.is_nullable,
  COALESCE(c.column_default, '') AS col_default,
  COALESCE(pk.is_pk, false) AS is_pk
FROM information_schema.columns c
LEFT JOIN (
  SELECT kcu.column_name, true AS is_pk
  FROM information_schema.table_constraints tc
  JOIN information_schema.key_column_usage kcu
    ON kcu.constraint_name = tc.constraint_name
   AND kcu.table_schema = tc.table_schema
  WHERE tc.constraint_type = 'PRIMARY KEY'
    AND tc.table_schema = $1
    AND tc.table_name = $2
) pk ON pk.column_name = c.column_name
WHERE c.table_schema = $1
  AND c.table_name = $2
ORDER BY c.ordinal_position;
`
	rows, err := p.SQL.QueryContext(ctx, q, schema, name)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var cols []db.Column
	for rows.Next() {
		var colName, dataType, isNullable, colDefault string
		var isPK bool
		if err := rows.Scan(&colName, &dataType, &isNullable, &colDefault, &isPK); err != nil {
			return nil, err
		}
		cols = append(cols, db.Column{
			Name:       colName,
			Type:       dataType,
			Nullable:   strings.EqualFold(isNullable, "YES"),
			Default:    colDefault,
			PrimaryKey: isPK,
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
	return `"` + strings.ReplaceAll(id, `"`, `""`) + `"`
}
