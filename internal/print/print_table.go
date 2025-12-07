package print

import (
	"fmt"
	"io"
	"strconv"
	"strings"

	"github.com/bgunnarsson/binsql/internal/db"
)

type Options struct {
	MaxWidth int // max width for each column, 0 = no limit
}

func RenderTable(w io.Writer, rows *db.Rows, opts Options) {
	if opts.MaxWidth <= 0 {
		opts.MaxWidth = 40
	}

	cols := len(rows.Columns)
	if cols == 0 {
		fmt.Fprintln(w, "(no columns)")
		return
	}

	// compute widths
	widths := make([]int, cols)
	for i, col := range rows.Columns {
		widths[i] = len(col.Name)
	}

	for _, r := range rows.Data {
		for i, cell := range r {
			s := formatCell(cell)
			if l := len(s); l > widths[i] {
				if l > opts.MaxWidth {
					l = opts.MaxWidth
				}
				widths[i] = l
			}
		}
	}

	// helpers
	sep := func(ch string) string {
		var b strings.Builder
		b.WriteString("+")
		for i := range widths {
			b.WriteString(strings.Repeat(ch, widths[i]+2))
			b.WriteString("+")
		}
		return b.String()
	}

	writeRow := func(cells []string) {
		var b strings.Builder
		b.WriteString("|")
		for i, c := range cells {
			cut := truncate(c, widths[i])
			b.WriteString(" ")
			b.WriteString(padRight(cut, widths[i]))
			b.WriteString(" |")
		}
		fmt.Fprintln(w, b.String())
	}

	// header
	fmt.Fprintln(w, sep("-"))
	header := make([]string, cols)
	for i, col := range rows.Columns {
		header[i] = col.Name
	}
	writeRow(header)
	fmt.Fprintln(w, sep("="))

	// data
	for _, r := range rows.Data {
		cells := make([]string, cols)
		for i, cell := range r {
			cells[i] = formatCell(cell)
		}
		writeRow(cells)
	}
	fmt.Fprintln(w, sep("-"))
}

func formatCell(v any) string {
	if v == nil {
		return "NULL"
	}
	switch t := v.(type) {
	case []byte:
		// heuristic: treat as string if printable, else show len
		s := string(t)
		if isPrintable(s) {
			return s
		}
		return fmt.Sprintf("<blob %d bytes>", len(t))
	case int64:
		return strconv.FormatInt(t, 10)
	case float64:
		return strconv.FormatFloat(t, 'f', -1, 64)
	default:
		return fmt.Sprint(t)
	}
}

func isPrintable(s string) bool {
	for _, r := range s {
		if r < 32 && r != '\n' && r != '\t' {
			return false
		}
	}
	return true
}

func padRight(s string, w int) string {
	if len(s) >= w {
		return s
	}
	return s + strings.Repeat(" ", w-len(s))
}

func truncate(s string, w int) string {
	if len(s) <= w {
		return s
	}
	if w <= 1 {
		return s[:w]
	}
	if w == 2 {
		return s[:2]
	}
	return s[:w-3] + "..."
}

