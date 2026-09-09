package render

import (
	"bytes"
	"encoding/json"
	"strings"
	"testing"

	"github.com/bgunnarsson/binsql/internal/db"
)

func sample() *db.Rows {
	return &db.Rows{
		Columns: []db.Column{{Name: "id", Type: "integer"}, {Name: "name", Type: "text"}},
		Data: []db.Row{
			{int64(1), "alice"},
			{int64(2), nil},
		},
	}
}

func renderTo(t *testing.T, rows *db.Rows, opts Options) string {
	t.Helper()
	var buf bytes.Buffer
	if err := Rows(&buf, Result{Rows: rows}, opts); err != nil {
		t.Fatalf("Rows() error: %v", err)
	}
	return buf.String()
}

func TestTableAlignment(t *testing.T) {
	got := renderTo(t, sample(), Options{Format: FormatTable})

	lines := strings.Split(strings.TrimRight(got, "\n"), "\n")
	// Every border and data line must be the same display width.
	want := width(lines[0])
	for i, l := range lines[:len(lines)-1] {
		if w := width(l); w != want {
			t.Errorf("line %d width = %d, want %d\n%q", i, w, want, l)
		}
	}
	if !strings.Contains(got, "NULL") {
		t.Error("expected NULL for a nil value")
	}
}

// A wide character occupies two display columns; the border must account for
// that or the table skews.
func TestTableAlignmentWithWideRunes(t *testing.T) {
	rows := &db.Rows{
		Columns: []db.Column{{Name: "name"}},
		Data:    []db.Row{{"日本語"}, {"ok"}},
	}
	got := renderTo(t, rows, Options{Format: FormatTable})

	lines := strings.Split(strings.TrimRight(got, "\n"), "\n")
	want := width(lines[0])
	for i, l := range lines[:len(lines)-1] {
		if w := width(l); w != want {
			t.Errorf("line %d width = %d, want %d\n%q", i, w, want, l)
		}
	}
}

// A newline inside a value must not break the table into extra lines.
func TestTableEscapesNewlines(t *testing.T) {
	rows := &db.Rows{
		Columns: []db.Column{{Name: "note"}},
		Data:    []db.Row{{"line1\nline2"}},
	}
	got := renderTo(t, rows, Options{Format: FormatTable})

	if n := strings.Count(got, "\n"); n != 6 { // 3 borders + header + data + footer
		t.Errorf("unexpected line count %d in:\n%s", n, got)
	}
	if !strings.Contains(got, `line1\nline2`) {
		t.Errorf("newline was not escaped:\n%s", got)
	}
}

func TestTableNoFooter(t *testing.T) {
	got := renderTo(t, sample(), Options{Format: FormatTable, NoFooter: true})
	if strings.Contains(got, "rows)") {
		t.Errorf("footer should be suppressed:\n%s", got)
	}
}

func TestJSONShape(t *testing.T) {
	var buf bytes.Buffer
	err := Rows(&buf, Result{Rows: sample(), DurationMS: 1.5}, Options{Format: FormatJSON})
	if err != nil {
		t.Fatal(err)
	}

	var out struct {
		Columns []db.Column      `json:"columns"`
		Rows    []map[string]any `json:"rows"`
		Count   int              `json:"row_count"`
		Total   int              `json:"total_rows"`
		Trunc   bool             `json:"truncated"`
	}
	if err := json.Unmarshal(buf.Bytes(), &out); err != nil {
		t.Fatalf("output is not valid JSON: %v\n%s", err, buf.String())
	}

	if out.Count != 2 || out.Total != 2 || out.Trunc {
		t.Errorf("unexpected counts: %+v", out)
	}
	// Numbers must survive as numbers, and NULL as JSON null.
	if got := out.Rows[0]["id"]; got != float64(1) {
		t.Errorf("id = %#v, want number 1", got)
	}
	if got, ok := out.Rows[1]["name"]; !ok || got != nil {
		t.Errorf("null name = %#v, want nil", got)
	}
}

func TestJSONTruncationIsReported(t *testing.T) {
	var buf bytes.Buffer
	if err := Rows(&buf, Result{Rows: sample()}, Options{Format: FormatJSON, MaxRows: 1}); err != nil {
		t.Fatal(err)
	}

	var out struct {
		Count int  `json:"row_count"`
		Total int  `json:"total_rows"`
		Trunc bool `json:"truncated"`
	}
	if err := json.Unmarshal(buf.Bytes(), &out); err != nil {
		t.Fatal(err)
	}
	if out.Count != 1 || out.Total != 2 || !out.Trunc {
		t.Errorf("truncation not reported: %+v", out)
	}
}

// Duplicate column names must not collide and silently drop a value.
func TestJSONDuplicateColumnNames(t *testing.T) {
	rows := &db.Rows{
		Columns: []db.Column{{Name: "id"}, {Name: "id"}},
		Data:    []db.Row{{int64(1), int64(2)}},
	}
	var buf bytes.Buffer
	if err := Rows(&buf, Result{Rows: rows}, Options{Format: FormatJSON}); err != nil {
		t.Fatal(err)
	}

	var out struct {
		Rows []map[string]any `json:"rows"`
	}
	if err := json.Unmarshal(buf.Bytes(), &out); err != nil {
		t.Fatal(err)
	}
	if len(out.Rows[0]) != 2 {
		t.Errorf("expected 2 distinct keys, got %#v", out.Rows[0])
	}
	if out.Rows[0]["id"] != float64(1) || out.Rows[0]["id:2"] != float64(2) {
		t.Errorf("unexpected keys: %#v", out.Rows[0])
	}
}

func TestCSVQuotesAndNulls(t *testing.T) {
	rows := &db.Rows{
		Columns: []db.Column{{Name: "a"}, {Name: "b"}},
		Data:    []db.Row{{"has,comma", nil}, {"has\nnewline", "x"}},
	}
	got := renderTo(t, rows, Options{Format: FormatCSV})

	if !strings.Contains(got, `"has,comma",`) {
		t.Errorf("comma not quoted:\n%s", got)
	}
	if !strings.Contains(got, "\"has\nnewline\"") {
		t.Errorf("newline not quoted:\n%s", got)
	}
}

func TestJSONLOneObjectPerLine(t *testing.T) {
	got := renderTo(t, sample(), Options{Format: FormatJSONL})

	lines := strings.Split(strings.TrimRight(got, "\n"), "\n")
	if len(lines) != 2 {
		t.Fatalf("expected 2 lines, got %d:\n%s", len(lines), got)
	}
	for _, l := range lines {
		var v map[string]any
		if err := json.Unmarshal([]byte(l), &v); err != nil {
			t.Errorf("line is not valid JSON: %q", l)
		}
	}
}

func TestMarkdownTable(t *testing.T) {
	got := renderTo(t, sample(), Options{Format: FormatMarkdown})

	lines := strings.Split(strings.TrimRight(got, "\n"), "\n")
	if len(lines) != 4 { // header, separator, 2 data rows
		t.Fatalf("expected 4 lines, got %d:\n%s", len(lines), got)
	}
	if lines[0] != "| id | name |" {
		t.Errorf("header = %q", lines[0])
	}
	if lines[1] != "| --- | --- |" {
		t.Errorf("separator = %q", lines[1])
	}
	// Every row must have the same number of pipes as the header, or the
	// table will not render.
	want := strings.Count(lines[0], "|")
	for i, l := range lines {
		if got := strings.Count(l, "|"); got != want {
			t.Errorf("line %d has %d pipes, want %d: %q", i, got, want, l)
		}
	}
}

func TestMarkdownEscapesPipes(t *testing.T) {
	rows := &db.Rows{
		Columns: []db.Column{{Name: "expr"}},
		Data:    []db.Row{{"a | b"}},
	}
	got := renderTo(t, rows, Options{Format: FormatMarkdown})
	if !strings.Contains(got, `a \| b`) {
		t.Errorf("pipe not escaped:\n%s", got)
	}
}

func TestParseFormat(t *testing.T) {
	for _, name := range []string{"table", "json", "jsonl", "ndjson", "csv", "tsv", "vertical", "md", ""} {
		if _, err := ParseFormat(name); err != nil {
			t.Errorf("ParseFormat(%q) failed: %v", name, err)
		}
	}
	if _, err := ParseFormat("yaml"); err == nil {
		t.Error("expected an error for an unknown format")
	}
}

func TestTruncate(t *testing.T) {
	tests := []struct {
		in   string
		w    int
		want string
	}{
		{"hello", 10, "hello"},
		{"hello", 5, "hello"},
		{"hello", 4, "hel…"},
		// Each CJK glyph is 2 columns wide, so only one fits alongside the
		// ellipsis without exceeding the 4-column budget.
		{"日本語です", 4, "日…"},
	}
	for _, tt := range tests {
		if got := truncate(tt.in, tt.w); got != tt.want {
			t.Errorf("truncate(%q, %d) = %q, want %q", tt.in, tt.w, got, tt.want)
		}
		if w := width(truncate(tt.in, tt.w)); w > tt.w {
			t.Errorf("truncate(%q, %d) is %d columns wide", tt.in, tt.w, w)
		}
	}
}

func TestBinaryValueEncoding(t *testing.T) {
	rows := &db.Rows{
		Columns: []db.Column{{Name: "blob"}},
		Data:    []db.Row{{[]byte{0x00, 0x01, 0xff}}},
	}

	if got := renderTo(t, rows, Options{Format: FormatTable}); !strings.Contains(got, "<binary 3 bytes>") {
		t.Errorf("table binary rendering:\n%s", got)
	}

	var buf bytes.Buffer
	if err := Rows(&buf, Result{Rows: rows}, Options{Format: FormatJSON}); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), "base64:") {
		t.Errorf("json binary rendering:\n%s", buf.String())
	}
}
