package render

import (
	"fmt"
	"io"
	"strings"

	"github.com/bgunnarsson/binsql/internal/db"
)

// StatementResult is the outcome of one statement in an exec run. Reads carry
// Rows; writes carry RowsAffected.
type StatementResult struct {
	SQL             string   `json:"sql"`
	Kind            string   `json:"kind"`
	RowsAffected    int64    `json:"rows_affected"`
	HasRowsAffected bool     `json:"-"`
	LastInsertID    int64    `json:"last_insert_id,omitempty"`
	DurationMS      float64  `json:"duration_ms"`
	Rows            *db.Rows `json:"-"`
	RowCount        int      `json:"row_count,omitempty"`
}

// ExecSummary is the wire shape of an exec run.
type ExecSummary struct {
	Statements        []StatementResult `json:"statements"`
	TotalRowsAffected int64             `json:"total_rows_affected"`
	Transaction       bool              `json:"transaction"`
	DryRun            bool              `json:"dry_run"`
	RolledBack        bool              `json:"rolled_back"`
	DurationMS        float64           `json:"duration_ms"`
}

// Exec writes the outcome of an exec run.
func Exec(w io.Writer, sum ExecSummary, opts Options) error {
	switch opts.Format {
	case FormatNone:
		return nil
	case FormatJSON, FormatJSONL:
		return writeJSON(w, execJSON(sum, opts), opts)
	}

	for i, st := range sum.Statements {
		if st.Rows != nil {
			if i > 0 {
				fmt.Fprintln(w)
			}
			if err := Rows(w, Result{Rows: st.Rows, DurationMS: st.DurationMS}, opts); err != nil {
				return err
			}
			continue
		}

		line := fmt.Sprintf("%s: ", strings.ToUpper(st.Kind))
		if st.HasRowsAffected {
			line += fmt.Sprintf("%d row%s affected", st.RowsAffected, plural(st.RowsAffected))
		} else {
			line += "ok"
		}
		if st.LastInsertID != 0 {
			line += fmt.Sprintf(", last insert id %d", st.LastInsertID)
		}
		line += fmt.Sprintf(" (%.1fms)", st.DurationMS)
		fmt.Fprintln(w, line)
	}

	if len(sum.Statements) > 1 {
		fmt.Fprintf(w, "\nTotal: %d statements, %d row%s affected (%.1fms)\n",
			len(sum.Statements), sum.TotalRowsAffected, plural(sum.TotalRowsAffected), sum.DurationMS)
	}
	if sum.RolledBack {
		fmt.Fprintln(w, "DRY RUN: transaction rolled back, nothing was committed.")
	} else if sum.Transaction {
		fmt.Fprintln(w, "Committed.")
	}
	return nil
}

// execJSON attaches result rows to the JSON form, which the struct tags omit
// because the human formats render them separately.
func execJSON(sum ExecSummary, opts Options) map[string]any {
	statements := make([]map[string]any, 0, len(sum.Statements))
	for _, st := range sum.Statements {
		entry := map[string]any{
			"sql":         st.SQL,
			"kind":        st.Kind,
			"duration_ms": st.DurationMS,
		}
		if st.HasRowsAffected {
			entry["rows_affected"] = st.RowsAffected
		}
		if st.LastInsertID != 0 {
			entry["last_insert_id"] = st.LastInsertID
		}
		if st.Rows != nil {
			rows := make([]map[string]any, 0, len(st.Rows.Data))
			for _, row := range st.Rows.Data {
				rows = append(rows, rowMap(st.Rows.Columns, row))
			}
			entry["columns"] = st.Rows.Columns
			entry["rows"] = rows
			entry["row_count"] = len(rows)
		}
		statements = append(statements, entry)
	}

	return map[string]any{
		"statements":          statements,
		"total_rows_affected": sum.TotalRowsAffected,
		"transaction":         sum.Transaction,
		"dry_run":             sum.DryRun,
		"rolled_back":         sum.RolledBack,
		"duration_ms":         sum.DurationMS,
	}
}

// List writes a simple one-column listing, e.g. table names.
func List(w io.Writer, header string, items []string, opts Options) error {
	switch opts.Format {
	case FormatNone:
		return nil
	case FormatJSON:
		return writeJSON(w, map[string]any{header: items, "count": len(items)}, opts)
	case FormatRaw, FormatJSONL:
		for _, it := range items {
			if _, err := fmt.Fprintln(w, it); err != nil {
				return err
			}
		}
		return nil
	}

	rows := &db.Rows{Columns: []db.Column{{Name: header}}}
	for _, it := range items {
		rows.Data = append(rows.Data, db.Row{it})
	}
	return Rows(w, Result{Rows: rows}, opts)
}

// TableSchema is one table's column list, for `describe` and `schema`.
type TableSchema struct {
	Table   string      `json:"table"`
	Columns []db.Column `json:"columns"`
}

// Table writes a single table definition. The JSON form is always one object,
// regardless of how many columns it has.
func Table(w io.Writer, table TableSchema, opts Options) error {
	switch opts.Format {
	case FormatNone:
		return nil
	case FormatJSON, FormatJSONL:
		return writeJSON(w, table, opts)
	}
	return schemaText(w, []TableSchema{table}, opts)
}

// Schema writes a set of table definitions. The JSON form is always a
// {"tables": [...]} envelope, so a consumer sees one shape whether the
// database has one table or fifty.
func Schema(w io.Writer, tables []TableSchema, opts Options) error {
	switch opts.Format {
	case FormatNone:
		return nil
	case FormatJSON, FormatJSONL:
		if tables == nil {
			tables = []TableSchema{}
		}
		return writeJSON(w, map[string]any{"tables": tables, "count": len(tables)}, opts)
	}
	return schemaText(w, tables, opts)
}

// schemaText renders table definitions in the human-facing formats.
func schemaText(w io.Writer, tables []TableSchema, opts Options) error {
	for i, t := range tables {
		if i > 0 {
			fmt.Fprintln(w)
		}
		if len(tables) > 1 || opts.Format == FormatMarkdown {
			fmt.Fprintf(w, "%s\n", t.Table)
		}
		rows := &db.Rows{
			Columns: []db.Column{
				{Name: "column"}, {Name: "type"}, {Name: "nullable"}, {Name: "key"}, {Name: "default"},
			},
		}
		for _, c := range t.Columns {
			key := ""
			if c.PrimaryKey {
				key = "PK"
			}
			rows.Data = append(rows.Data, db.Row{
				c.Name, c.Type, yesNo(c.Nullable), key, c.Default,
			})
		}
		if err := Rows(w, Result{Rows: rows}, opts); err != nil {
			return err
		}
	}
	return nil
}

// Error writes an error in a shape that matches the selected format, so a
// caller parsing JSON gets JSON on failure too.
func Error(w io.Writer, err error, opts Options) {
	if opts.Format == FormatJSON || opts.Format == FormatJSONL {
		_ = writeJSON(w, map[string]any{"error": err.Error()}, opts)
		return
	}
	fmt.Fprintln(w, "error:", err)
}

func yesNo(b bool) string {
	if b {
		return "yes"
	}
	return "no"
}

func plural(n int64) string {
	if n == 1 {
		return ""
	}
	return "s"
}
