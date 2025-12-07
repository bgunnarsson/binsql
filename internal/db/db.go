package db

import (
	"context"
)

type Column struct {
	Name string
	Type string
}

type Row []any

type Rows struct {
	Columns []Column
	Data    []Row
}

type DB interface {
	Close() error
	ListTables(ctx context.Context) ([]string, error)
	DescribeTable(ctx context.Context, table string) ([]Column, error)
	Query(ctx context.Context, sql string, args ...any) (*Rows, error)
}

