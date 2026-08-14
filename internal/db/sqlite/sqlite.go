package sqlite

import (
	"context"
	"database/sql"
	"fmt"
	"strings"
	"time"

	_ "modernc.org/sqlite" // register driver

	"github.com/bgunnarsson/binsql/internal/db"
	"github.com/bgunnarsson/binsql/internal/db/sqlcore"
)

type SqliteDB struct {
	*sqlcore.Core
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

	return &SqliteDB{Core: &sqlcore.Core{SQL: sqldb, Conv: convert}}, nil
}

// convert leaves values alone: modernc/sqlite already returns int64, float64,
// string and []byte, and the renderers know how to present each.
func convert(v any, _ string) any { return v }

func (s *SqliteDB) Dialect() db.Dialect {
	return db.Dialect{
		Name:        "sqlite",
		Placeholder: func(int) string { return "?" },
		QuoteIdent:  quoteIdent,
		SelectLimit: selectLimit,
	}
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

	rows, err := s.SQL.QueryContext(ctx, q)
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
	rows, err := s.SQL.QueryContext(ctx, q)
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
			Name:       name,
			Type:       strings.ToLower(ctype),
			Nullable:   notnull == 0,
			Default:    dflt.String,
			PrimaryKey: pk > 0,
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

// very basic identifier quoting – enough for sqlite
func quoteIdent(id string) string {
	return `"` + strings.ReplaceAll(id, `"`, `""`) + `"`
}
