package mssql

import (
	"context"
	"database/sql"
	"fmt"
	"strings"
	"time"

	"github.com/microsoft/go-mssqldb/azuread"
	_ "github.com/microsoft/go-mssqldb"

	"github.com/bgunnarsson/binsql/internal/db"
)

type MssqlDB struct {
	db *sql.DB
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

	return &MssqlDB{db: sqldb}, nil
}

// --- db.DB implementation ---

func (m *MssqlDB) Close() error {
	if m.db == nil {
		return nil
	}
	return m.db.Close()
}

func (m *MssqlDB) ListTables(ctx context.Context) ([]string, error) {
	const q = `
SELECT TABLE_SCHEMA + '.' + TABLE_NAME AS name
FROM INFORMATION_SCHEMA.TABLES
WHERE TABLE_TYPE = 'BASE TABLE'
ORDER BY TABLE_SCHEMA, TABLE_NAME;
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

// DescribeTable returns column name + data type.
// Accepts either "table" or "schema.table".
func (m *MssqlDB) DescribeTable(ctx context.Context, table string) ([]db.Column, error) {
	schema := "dbo"
	name := table
	if dot := strings.Index(table, "."); dot != -1 {
		schema = table[:dot]
		name = table[dot+1:]
	}

	const q = `
SELECT COLUMN_NAME, DATA_TYPE
FROM INFORMATION_SCHEMA.COLUMNS
WHERE TABLE_SCHEMA = @p1 AND TABLE_NAME = @p2
ORDER BY ORDINAL_POSITION;
`
	rows, err := m.db.QueryContext(ctx, q, schema, name)
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

func (m *MssqlDB) Query(ctx context.Context, sqlQuery string, args ...any) (*db.Rows, error) {
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
            dbType := ""
            if i < len(colTypes) && colTypes[i] != nil {
                dbType = strings.ToLower(colTypes[i].DatabaseTypeName())
            }

            switch x := v.(type) {
            case []byte:
                // NEVER string() binary; it wrecks the table.
                switch dbType {
                case "uniqueidentifier":
                    values[i] = formatUniqueIdentifier(x)
                default:
                    // safe hex representation for any other binary
                    values[i] = fmt.Sprintf("0x%x", x)
                }

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

