package mssql

import (
	"context"
	"database/sql"
	"fmt"
	"strconv"
	"strings"
	"time"

	_ "github.com/microsoft/go-mssqldb"
	"github.com/microsoft/go-mssqldb/azuread"

	"github.com/bgunnarsson/binsql/internal/db"
	"github.com/bgunnarsson/binsql/internal/db/sqlcore"
)

type MssqlDB struct {
	*sqlcore.Core
}

// Open opens a MSSQL connection.
// If the DSN contains "fedauth=", we use the Azure AD driver (azuresql)
// so things like ActiveDirectoryInteractive / AzCli work.
func Open(dsn string) (*MssqlDB, error) {
	if dsn == "" {
		return nil, fmt.Errorf("empty mssql DSN")
	}

	driverName := "sqlserver"
	if strings.Contains(strings.ToLower(dsn), "fedauth=") {
		driverName = azuread.DriverName // "azuresql"
	}

	sqldb, err := sql.Open(driverName, dsn)
	if err != nil {
		return nil, err
	}

	// small CLI defaults
	sqldb.SetMaxOpenConns(4)
	sqldb.SetMaxIdleConns(4)
	sqldb.SetConnMaxLifetime(5 * time.Minute)

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	if err := sqldb.PingContext(ctx); err != nil {
		sqldb.Close()
		return nil, err
	}

	return &MssqlDB{Core: &sqlcore.Core{SQL: sqldb, Conv: convert}}, nil
}

func convert(v any, dbType string) any {
	switch x := v.(type) {
	case []byte:
		// NEVER string() binary; it wrecks the table.
		switch dbType {
		case "uniqueidentifier":
			return formatUniqueIdentifier(x)
		default:
			// safe hex representation for any other binary
			return fmt.Sprintf("0x%x", x)
		}
	case time.Time:
		return x.Format(time.RFC3339Nano)
	default:
		return x
	}
}

func (m *MssqlDB) Dialect() db.Dialect {
	return db.Dialect{
		Name:        "mssql",
		Placeholder: func(n int) string { return "@p" + strconv.Itoa(n) },
		QuoteIdent:  quoteIdent,
		SelectLimit: selectLimit,
	}
}

func (m *MssqlDB) ListTables(ctx context.Context) ([]string, error) {
	const q = `
SELECT TABLE_SCHEMA + '.' + TABLE_NAME AS name
FROM INFORMATION_SCHEMA.TABLES
WHERE TABLE_TYPE IN ('BASE TABLE', 'VIEW')
ORDER BY TABLE_SCHEMA, TABLE_NAME;
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

// DescribeTable returns column name, type, nullability, default and primary
// key membership. Accepts either "table" or "schema.table".
func (m *MssqlDB) DescribeTable(ctx context.Context, table string) ([]db.Column, error) {
	schema := "dbo"
	name := table
	if dot := strings.Index(table, "."); dot != -1 {
		schema = table[:dot]
		name = table[dot+1:]
	}

	const q = `
SELECT
  c.COLUMN_NAME,
  c.DATA_TYPE + CASE
    WHEN c.CHARACTER_MAXIMUM_LENGTH = -1 THEN '(max)'
    WHEN c.CHARACTER_MAXIMUM_LENGTH IS NOT NULL
      THEN '(' + CAST(c.CHARACTER_MAXIMUM_LENGTH AS varchar(20)) + ')'
    ELSE ''
  END AS type,
  c.IS_NULLABLE,
  COALESCE(c.COLUMN_DEFAULT, '') AS col_default,
  CASE WHEN pk.COLUMN_NAME IS NULL THEN 0 ELSE 1 END AS is_pk
FROM INFORMATION_SCHEMA.COLUMNS c
LEFT JOIN (
  SELECT kcu.COLUMN_NAME
  FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc
  JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE kcu
    ON kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME
   AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA
  WHERE tc.CONSTRAINT_TYPE = 'PRIMARY KEY'
    AND tc.TABLE_SCHEMA = @p1
    AND tc.TABLE_NAME = @p2
) pk ON pk.COLUMN_NAME = c.COLUMN_NAME
WHERE c.TABLE_SCHEMA = @p1
  AND c.TABLE_NAME = @p2
ORDER BY c.ORDINAL_POSITION;
`
	rows, err := m.SQL.QueryContext(ctx, q, schema, name)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var cols []db.Column
	for rows.Next() {
		var colName, dataType, isNullable, colDefault string
		var isPK int
		if err := rows.Scan(&colName, &dataType, &isNullable, &colDefault, &isPK); err != nil {
			return nil, err
		}
		cols = append(cols, db.Column{
			Name:       colName,
			Type:       dataType,
			Nullable:   strings.EqualFold(isNullable, "YES"),
			Default:    colDefault,
			PrimaryKey: isPK == 1,
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

// selectLimit uses TOP because SQL Server has no LIMIT clause.
func selectLimit(table, where string, n int) string {
	q := "SELECT "
	if n > 0 {
		q += fmt.Sprintf("TOP (%d) ", n)
	}
	q += "* FROM " + table
	if where != "" {
		q += " WHERE " + where
	}
	return q
}

func quoteIdent(id string) string {
	return "[" + strings.ReplaceAll(id, "]", "]]") + "]"
}

func formatUniqueIdentifier(b []byte) string {
	if len(b) != 16 {
		return fmt.Sprintf("%x", b)
	}

	return fmt.Sprintf("%02x%02x%02x%02x-%02x%02x-%02x%02x-%02x%02x-%02x%02x%02x%02x%02x%02x",
		b[3], b[2], b[1], b[0],
		b[5], b[4],
		b[7], b[6],
		b[8], b[9],
		b[10], b[11], b[12], b[13], b[14], b[15],
	)
}
