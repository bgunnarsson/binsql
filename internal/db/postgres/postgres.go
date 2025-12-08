package postgres

import (
	"context"
	"database/sql"
	"fmt"
	"strings"
	"time"

	_ "github.com/jackc/pgx/v5/stdlib" // register pgx stdlib driver

	"github.com/bgunnarsson/binsql/internal/db"
)

type PostgresDB struct {
	db *sql.DB
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

	return &PostgresDB{db: sqldb}, nil
}

func (p *PostgresDB) Close() error {
	if p.db == nil {
		return nil
	}
	return p.db.Close()
}

func (p *PostgresDB) ListTables(ctx context.Context) ([]string, error) {
	const q = `
SELECT table_schema || '.' || table_name AS name
FROM information_schema.tables
WHERE table_type = 'BASE TABLE'
  AND table_schema NOT IN ('pg_catalog', 'information_schema')
ORDER BY table_schema, table_name;
`
	rows, err := p.db.QueryContext(ctx, q)
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

// DescribeTable returns column name + data type.
// Accepts either "table" or "schema.table".
func (p *PostgresDB) DescribeTable(ctx context.Context, table string) ([]db.Column, error) {
	schema := "public"
	name := table
	if dot := strings.Index(table, "."); dot != -1 {
		schema = table[:dot]
		name = table[dot+1:]
	}

	const q = `
SELECT column_name, data_type
FROM information_schema.columns
WHERE table_schema = $1
  AND table_name = $2
ORDER BY ordinal_position;
`
	rows, err := p.db.QueryContext(ctx, q, schema, name)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var cols []db.Column
	for rows.Next() {
		var colName, dataType string
		if err := rows.Scan(&colName, &dataType); err != nil {
			return nil, err
		}
		cols = append(cols, db.Column{
			Name: colName,
			Type: dataType,
		})
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	return cols, nil
}

func (p *PostgresDB) Query(ctx context.Context, sqlQuery string, args ...any) (*db.Rows, error) {
	rows, err := p.db.QueryContext(ctx, sqlQuery, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	colNames, err := rows.Columns()
	if err != nil {
		return nil, err
	}

	colTypes, err := rows.ColumnTypes()
	if err != nil {
		return nil, err
	}

	header := make([]db.Column, len(colNames))
	for i, name := range colNames {
		typ := ""
		if i < len(colTypes) && colTypes[i] != nil {
			typ = strings.ToLower(colTypes[i].DatabaseTypeName())
		}
		header[i] = db.Column{
			Name: name,
			Type: typ,
		}
	}

	var data []db.Row
	for rows.Next() {
		values := make([]any, len(colNames))
		ptrs := make([]any, len(colNames))
		for i := range values {
			ptrs[i] = &values[i]
		}

		if err := rows.Scan(ptrs...); err != nil {
			return nil, err
		}

		for i, v := range values {
			switch x := v.(type) {
			case []byte:
				values[i] = string(x)
			case time.Time:
				values[i] = x.Format(time.RFC3339Nano)
			default:
				values[i] = x
			}
		}

		data = append(data, db.Row(values))
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}

	return &db.Rows{
		Columns: header,
		Data:    data,
	}, nil
}

