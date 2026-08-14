package db

import (
	"context"
	"errors"
	"strings"
)

// ErrRollback can be returned from the callback passed to DB.InTx to roll the
// transaction back without surfacing an error to the caller. It backs the
// `--dry-run` flag.
var ErrRollback = errors.New("rollback requested")

type Column struct {
	Name       string `json:"name"`
	Type       string `json:"type"`
	Nullable   bool   `json:"nullable"`
	Default    string `json:"default,omitempty"`
	PrimaryKey bool   `json:"primary_key,omitempty"`
}

type Row []any

type Rows struct {
	Columns []Column
	Data    []Row
}

// Result describes the outcome of a statement that returns no rows.
// Not every driver reports both numbers, hence the Has* flags.
type Result struct {
	RowsAffected    int64
	LastInsertID    int64
	HasRowsAffected bool
	HasLastInsertID bool
}

// Executor is the subset of a connection that runs statements. Both a plain
// connection and an open transaction satisfy it.
type Executor interface {
	Query(ctx context.Context, sql string, args ...any) (*Rows, error)
	Exec(ctx context.Context, sql string, args ...any) (*Result, error)
}

// Dialect carries the per-driver syntax the generic command layer needs.
type Dialect struct {
	Name string
	// Placeholder renders the bind marker for the n-th argument (1-based).
	Placeholder func(n int) string
	// QuoteIdent quotes a single identifier part.
	QuoteIdent func(string) string
	// SelectLimit builds a row-limited SELECT. table is already quoted and
	// where is a bare predicate (no WHERE keyword) or empty.
	SelectLimit func(table, where string, n int) string
}

// QuoteTable quotes a possibly schema-qualified table name.
func (d Dialect) QuoteTable(name string) string {
	quote := d.QuoteIdent
	if quote == nil {
		return name
	}
	parts := strings.Split(name, ".")
	for i, p := range parts {
		parts[i] = quote(p)
	}
	return strings.Join(parts, ".")
}

type DB interface {
	Executor
	Close() error
	ListTables(ctx context.Context) ([]string, error)
	DescribeTable(ctx context.Context, table string) ([]Column, error)
	Dialect() Dialect
	// InTx runs fn inside a transaction, committing on nil and rolling back
	// on any error. ErrRollback rolls back but reports success.
	InTx(ctx context.Context, fn func(Executor) error) error
}
