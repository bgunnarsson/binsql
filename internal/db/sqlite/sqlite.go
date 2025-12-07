package sqlite

import (
	"context"
	"database/sql"
	"fmt"
	"strings"
	"time"

	_ "modernc.org/sqlite" // register driver

	"github.com/bgunnarsson/binsql/internal/db"
)

type SqliteDB struct {
	db *sql.DB
}

func Open(path string) (*SqliteDB, error) {
	// Keep it simple: open by plain path, then enable pragmas explicitly.
	sqldb, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, err
	}

	// Sane defaults for a CLI tool.
	sqldb.SetMaxOpenConns(1)
	sqldb.SetConnMaxLifetime(5 * time.Minute)

	// Enable foreign keys.
	if _, err := sqldb.Exec(`PRAGMA foreign_keys = ON;`); err != nil {
		_ = sqldb.Close()
		return nil, err
	}

	return &SqliteDB{db: sqldb}, nil
}

func (s *SqliteDB) Close() error {
	return s.db.Close()
}

func (s *SqliteDB) ListTables(ctx context.Context) ([]string, error) {
	// Use sqlite_master (works everywhere), include tables + views,
	// hide internal sqlite_% objects.
	const q = `
		SELECT name
		FROM sqlite_master
		WHERE type IN ('table', 'view')
		  AND name NOT LIKE 'sqlite_%'
		ORDER BY lower(name);
	`

	rows, err := s.db.QueryContext(ctx, q)
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
	return out, rows.Err()
}

func (s *SqliteDB) DescribeTable(ctx context.Context, table string) ([]db.Column, error) {
	q := fmt.Sprintf("PRAGMA table_info(%s);", quoteIdent(table))
	rows, err := s.db.QueryContext(ctx, q)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var cols []db.Column
	for rows.Next() {
		var cid int
		var name, ctype string
		var notnull, pk int
		var dflt sql.NullString
		if err := rows.Scan(&cid, &name, &ctype, &notnull, &dflt, &pk); err != nil {
			return nil, err
		}
		cols = append(cols, db.Column{
			Name: name,
			Type: ctype,
		})
	}
	return cols, rows.Err()
}

func (s *SqliteDB) Query(ctx context.Context, sqlStr string, args ...any) (*db.Rows, error) {
	rows, err := s.db.QueryContext(ctx, sqlStr, args...)
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
	for i := range colNames {
		header[i] = db.Column{
			Name: colNames[i],
			Type: strings.ToUpper(colTypes[i].DatabaseTypeName()),
		}
	}

	var data []db.Row
	for rows.Next() {
		raw := make([]any, len(colNames))
		ptrs := make([]any, len(colNames))
		for i := range raw {
			ptrs[i] = &raw[i]
		}
		if err := rows.Scan(ptrs...); err != nil {
			return nil, err
		}
		data = append(data, db.Row(raw))
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}

	return &db.Rows{
		Columns: header,
		Data:    data,
	}, nil
}

// very basic identifier quoting – enough for sqlite
func quoteIdent(id string) string {
	return `"` + strings.ReplaceAll(id, `"`, `""`) + `"`
}
