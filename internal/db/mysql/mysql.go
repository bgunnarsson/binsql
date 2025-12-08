package mysql

import (
	"context"
	"database/sql"
	"fmt"
	"strings"
	"time"

	_ "github.com/go-sql-driver/mysql"

	"github.com/bgunnarsson/binsql/internal/db"
)

type MysqlDB struct {
	db *sql.DB
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

	return &MysqlDB{db: sqldb}, nil
}

// --- db.DB implementation ---

func (m *MysqlDB) Close() error {
	if m.db == nil {
		return nil
	}
	return m.db.Close()
}

func (m *MysqlDB) ListTables(ctx context.Context) ([]string, error) {
	const q = `
SELECT table_name
FROM information_schema.tables
WHERE table_type = 'BASE TABLE'
  AND table_schema = DATABASE()
ORDER BY table_name;
`
	rows, err := m.db.QueryContext(ctx, q)
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
	const q = `
SELECT column_name, data_type
FROM information_schema.columns
WHERE table_schema = DATABASE()
  AND table_name = ?
ORDER BY ordinal_position;
`
	rows, err := m.db.QueryContext(ctx, q, table)
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

func (m *MysqlDB) Query(ctx context.Context, sqlQuery string, args ...any) (*db.Rows, error) {
	rows, err := m.db.QueryContext(ctx, sqlQuery, args...)
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
				// MySQL returns TEXT/VARCHAR as []byte
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

