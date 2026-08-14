// Package sqlcore holds the database/sql plumbing shared by every adapter:
// scanning result sets, running statements and transactions. Adapters supply
// only a Convert func for their driver-specific value quirks.
package sqlcore

import (
	"context"
	"database/sql"
	"errors"
	"strings"

	"github.com/bgunnarsson/binsql/internal/db"
)

// Convert normalizes one raw driver value. dbType is the lower-cased database
// type name of the column, which some drivers need to disambiguate []byte.
type Convert func(v any, dbType string) any

// runner is satisfied by both *sql.DB and *sql.Tx.
type runner interface {
	QueryContext(ctx context.Context, query string, args ...any) (*sql.Rows, error)
	ExecContext(ctx context.Context, query string, args ...any) (sql.Result, error)
}

// Core implements the statement-running half of db.DB on top of a *sql.DB.
// Adapters embed it and add their own catalog queries and dialect.
type Core struct {
	SQL  *sql.DB
	Conv Convert
}

func (c *Core) Close() error {
	if c.SQL == nil {
		return nil
	}
	return c.SQL.Close()
}

func (c *Core) Query(ctx context.Context, query string, args ...any) (*db.Rows, error) {
	return queryWith(ctx, c.SQL, c.Conv, query, args...)
}

func (c *Core) Exec(ctx context.Context, query string, args ...any) (*db.Result, error) {
	return execWith(ctx, c.SQL, query, args...)
}

func (c *Core) InTx(ctx context.Context, fn func(db.Executor) error) error {
	tx, err := c.SQL.BeginTx(ctx, nil)
	if err != nil {
		return err
	}

	if err := fn(&txExecutor{tx: tx, conv: c.Conv}); err != nil {
		_ = tx.Rollback()
		if errors.Is(err, db.ErrRollback) {
			return nil
		}
		return err
	}
	return tx.Commit()
}

type txExecutor struct {
	tx   *sql.Tx
	conv Convert
}

func (t *txExecutor) Query(ctx context.Context, query string, args ...any) (*db.Rows, error) {
	return queryWith(ctx, t.tx, t.conv, query, args...)
}

func (t *txExecutor) Exec(ctx context.Context, query string, args ...any) (*db.Result, error) {
	return execWith(ctx, t.tx, query, args...)
}

func queryWith(ctx context.Context, r runner, conv Convert, query string, args ...any) (*db.Rows, error) {
	rows, err := r.QueryContext(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	return Scan(rows, conv)
}

func execWith(ctx context.Context, r runner, query string, args ...any) (*db.Result, error) {
	res, err := r.ExecContext(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	return toResult(res), nil
}

// Scan drains a *sql.Rows into db.Rows, applying conv to every value.
func Scan(rows *sql.Rows, conv Convert) (*db.Rows, error) {
	colNames, err := rows.Columns()
	if err != nil {
		return nil, err
	}
	colTypes, err := rows.ColumnTypes()
	if err != nil {
		return nil, err
	}

	header := make([]db.Column, len(colNames))
	dbTypes := make([]string, len(colNames))
	for i, name := range colNames {
		if i < len(colTypes) && colTypes[i] != nil {
			dbTypes[i] = strings.ToLower(colTypes[i].DatabaseTypeName())
			if nullable, ok := colTypes[i].Nullable(); ok {
				header[i].Nullable = nullable
			}
		}
		header[i].Name = name
		header[i].Type = dbTypes[i]
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
		if conv != nil {
			for i, v := range values {
				values[i] = conv(v, dbTypes[i])
			}
		}
		data = append(data, db.Row(values))
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}

	return &db.Rows{Columns: header, Data: data}, nil
}

func toResult(res sql.Result) *db.Result {
	out := &db.Result{}
	if n, err := res.RowsAffected(); err == nil {
		out.RowsAffected, out.HasRowsAffected = n, true
	}
	// Postgres and SQL Server report an error here rather than a value.
	if id, err := res.LastInsertId(); err == nil && id != 0 {
		out.LastInsertID, out.HasLastInsertID = id, true
	}
	return out
}
