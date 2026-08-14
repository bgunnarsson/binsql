package render

import (
	"encoding/csv"
	"encoding/json"
	"fmt"
	"io"
	"strings"

	"github.com/rivo/uniseg"

	"github.com/bgunnarsson/binsql/internal/db"
)

// Result carries a result set plus the metadata the JSON formats expose.
type Result struct {
	Rows       *db.Rows
	DurationMS float64
	Truncated  bool
	TotalRows  int
}

// Rows writes a result set in the requested format.
func Rows(w io.Writer, res Result, opts Options) error {
	if res.Rows == nil {
		res.Rows = &db.Rows{}
	}

	switch opts.Format {
	case FormatJSON:
		return rowsJSON(w, res, opts)
	case FormatJSONL:
		return rowsJSONL(w, res, opts)
	case FormatCSV:
		return rowsSeparated(w, res, opts, ',')
	case FormatTSV:
		return rowsSeparated(w, res, opts, '\t')
	case FormatVertical:
		return rowsVertical(w, res, opts)
	case FormatMarkdown:
		return rowsMarkdown(w, res, opts)
	case FormatRaw:
		return rowsRaw(w, res, opts)
	case FormatNone:
		return nil
	default:
		return rowsTable(w, res, opts)
	}
}

// visible returns the rows to print, honouring MaxRows.
func visible(res Result, opts Options) []db.Row {
	data := res.Rows.Data
	if opts.MaxRows > 0 && len(data) > opts.MaxRows {
		return data[:opts.MaxRows]
	}
	return data
}

func rowsTable(w io.Writer, res Result, opts Options) error {
	cols := res.Rows.Columns
	if len(cols) == 0 {
		_, err := fmt.Fprintln(w, "(no columns)")
		return err
	}

	maxWidth := opts.MaxWidth
	if maxWidth <= 0 {
		maxWidth = 40
	}
	data := visible(res, opts)

	widths := make([]int, len(cols))
	for i, col := range cols {
		widths[i] = min(width(col.Name), maxWidth)
	}
	cells := make([][]string, len(data))
	for r, row := range data {
		cells[r] = make([]string, len(cols))
		for i := range cols {
			var v any
			if i < len(row) {
				v = row[i]
			}
			s := Text(v, opts.nullText())
			cells[r][i] = s
			if l := min(width(s), maxWidth); l > widths[i] {
				widths[i] = l
			}
		}
	}

	sep := func(ch string) string {
		var b strings.Builder
		b.WriteString("+")
		for _, wd := range widths {
			b.WriteString(strings.Repeat(ch, wd+2))
			b.WriteString("+")
		}
		return b.String()
	}
	writeRow := func(row []string) {
		var b strings.Builder
		b.WriteString("|")
		for i, c := range row {
			b.WriteString(" ")
			b.WriteString(pad(truncate(c, widths[i]), widths[i]))
			b.WriteString(" |")
		}
		fmt.Fprintln(w, b.String())
	}

	fmt.Fprintln(w, sep("-"))
	if !opts.NoHeader {
		header := make([]string, len(cols))
		for i, col := range cols {
			header[i] = col.Name
		}
		writeRow(header)
		fmt.Fprintln(w, sep("="))
	}
	for _, row := range cells {
		writeRow(row)
	}
	fmt.Fprintln(w, sep("-"))

	if !opts.NoFooter {
		fmt.Fprintf(w, "(%s)\n", rowSummary(res, len(data)))
	}
	return nil
}

func rowsMarkdown(w io.Writer, res Result, opts Options) error {
	cols := res.Rows.Columns
	if len(cols) == 0 {
		_, err := fmt.Fprintln(w, "(no columns)")
		return err
	}

	names := make([]string, len(cols))
	for i, col := range cols {
		names[i] = escapePipes(col.Name)
	}
	fmt.Fprintf(w, "| %s |\n", strings.Join(names, " | "))
	// Repeat already supplies the trailing pipe for each column.
	fmt.Fprintf(w, "|%s\n", strings.Repeat(" --- |", len(cols)))

	for _, row := range visible(res, opts) {
		out := make([]string, len(cols))
		for i := range cols {
			var v any
			if i < len(row) {
				v = row[i]
			}
			out[i] = escapePipes(Text(v, opts.nullText()))
		}
		fmt.Fprintf(w, "| %s |\n", strings.Join(out, " | "))
	}
	return nil
}

func rowsVertical(w io.Writer, res Result, opts Options) error {
	cols := res.Rows.Columns
	data := visible(res, opts)
	if len(data) == 0 {
		_, err := fmt.Fprintln(w, "(0 rows)")
		return err
	}

	nameWidth := 0
	for _, col := range cols {
		nameWidth = max(nameWidth, width(col.Name))
	}

	for i, row := range data {
		fmt.Fprintf(w, "*************************** %d. row ***************************\n", i+1)
		for c, col := range cols {
			var v any
			if c < len(row) {
				v = row[c]
			}
			fmt.Fprintf(w, "%s%s: %s\n",
				strings.Repeat(" ", nameWidth-width(col.Name)), col.Name,
				Text(v, opts.nullText()))
		}
	}
	if !opts.NoFooter {
		fmt.Fprintf(w, "(%s)\n", rowSummary(res, len(data)))
	}
	return nil
}

func rowsRaw(w io.Writer, res Result, opts Options) error {
	for _, row := range visible(res, opts) {
		out := make([]string, len(row))
		for i, v := range row {
			out[i] = Text(v, opts.nullText())
		}
		if _, err := fmt.Fprintln(w, strings.Join(out, "\t")); err != nil {
			return err
		}
	}
	return nil
}

func rowsSeparated(w io.Writer, res Result, opts Options, comma rune) error {
	cw := csv.NewWriter(w)
	cw.Comma = comma

	if !opts.NoHeader && len(res.Rows.Columns) > 0 {
		header := make([]string, len(res.Rows.Columns))
		for i, col := range res.Rows.Columns {
			header[i] = col.Name
		}
		if err := cw.Write(header); err != nil {
			return err
		}
	}

	for _, row := range visible(res, opts) {
		out := make([]string, len(res.Rows.Columns))
		for i := range res.Rows.Columns {
			var v any
			if i < len(row) {
				v = row[i]
			}
			// CSV quotes embedded newlines correctly, so keep values verbatim.
			s, isNull := text(v)
			if isNull {
				s = ""
			}
			out[i] = s
		}
		if err := cw.Write(out); err != nil {
			return err
		}
	}

	cw.Flush()
	return cw.Error()
}

// jsonRows is the wire shape of `-o json` for a result set.
type jsonRows struct {
	Columns    []db.Column      `json:"columns"`
	Rows       []map[string]any `json:"rows"`
	RowCount   int              `json:"row_count"`
	TotalRows  int              `json:"total_rows"`
	Truncated  bool             `json:"truncated"`
	DurationMS float64          `json:"duration_ms"`
}

func rowsJSON(w io.Writer, res Result, opts Options) error {
	data := visible(res, opts)
	out := jsonRows{
		Columns:    res.Rows.Columns,
		Rows:       make([]map[string]any, 0, len(data)),
		RowCount:   len(data),
		TotalRows:  totalRows(res),
		Truncated:  res.Truncated || len(data) < len(res.Rows.Data),
		DurationMS: res.DurationMS,
	}
	if out.Columns == nil {
		out.Columns = []db.Column{}
	}
	for _, row := range data {
		out.Rows = append(out.Rows, rowMap(res.Rows.Columns, row))
	}
	return writeJSON(w, out, opts)
}

func rowsJSONL(w io.Writer, res Result, opts Options) error {
	enc := json.NewEncoder(w)
	for _, row := range visible(res, opts) {
		if err := enc.Encode(rowMap(res.Rows.Columns, row)); err != nil {
			return err
		}
	}
	return nil
}

// rowMap keys a row by column name, disambiguating duplicate names as
// "name:2", "name:3" so no value is silently dropped.
func rowMap(cols []db.Column, row db.Row) map[string]any {
	out := make(map[string]any, len(cols))
	for i, col := range cols {
		var v any
		if i < len(row) {
			v = row[i]
		}
		key := col.Name
		if key == "" {
			key = fmt.Sprintf("column_%d", i+1)
		}
		if _, clash := out[key]; clash {
			for n := 2; ; n++ {
				alt := fmt.Sprintf("%s:%d", key, n)
				if _, taken := out[alt]; !taken {
					key = alt
					break
				}
			}
		}
		out[key] = jsonValue(v)
	}
	return out
}

func writeJSON(w io.Writer, v any, opts Options) error {
	enc := json.NewEncoder(w)
	if opts.Pretty {
		enc.SetIndent("", "  ")
	}
	return enc.Encode(v)
}

func totalRows(res Result) int {
	if res.TotalRows > 0 {
		return res.TotalRows
	}
	return len(res.Rows.Data)
}

func rowSummary(res Result, shown int) string {
	total := totalRows(res)
	s := fmt.Sprintf("%d row", total)
	if total != 1 {
		s += "s"
	}
	if shown < total {
		s = fmt.Sprintf("%d of %s shown", shown, s)
	}
	if res.DurationMS > 0 {
		s += fmt.Sprintf(", %.1fms", res.DurationMS)
	}
	return s
}

// --- width helpers --------------------------------------------------------

// width measures display columns, so CJK and emoji do not skew the layout.
func width(s string) int { return uniseg.StringWidth(s) }

// pad right-pads s to w display columns.
func pad(s string, w int) string {
	if d := w - width(s); d > 0 {
		return s + strings.Repeat(" ", d)
	}
	return s
}

// truncate cuts a string to at most w display columns, adding an ellipsis.
func truncate(s string, w int) string {
	if width(s) <= w {
		return s
	}
	if w <= 1 {
		return cut(s, w)
	}
	return cut(s, w-1) + "…"
}

// cut returns the longest prefix of s that fits in w display columns.
func cut(s string, w int) string {
	if w <= 0 {
		return ""
	}
	var b strings.Builder
	used := 0
	gr := uniseg.NewGraphemes(s)
	for gr.Next() {
		cluster := gr.Str()
		cw := uniseg.StringWidth(cluster)
		if used+cw > w {
			break
		}
		b.WriteString(cluster)
		used += cw
	}
	return b.String()
}

func escapePipes(s string) string { return strings.ReplaceAll(s, "|", `\|`) }
