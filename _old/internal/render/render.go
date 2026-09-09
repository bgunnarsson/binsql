// Package render turns query results into the output formats the CLI offers.
package render

import (
	"fmt"
	"strings"
)

// Format names an output encoding.
type Format string

const (
	FormatTable    Format = "table"
	FormatJSON     Format = "json"
	FormatJSONL    Format = "jsonl"
	FormatCSV      Format = "csv"
	FormatTSV      Format = "tsv"
	FormatVertical Format = "vertical"
	FormatMarkdown Format = "markdown"
	FormatRaw      Format = "raw"
	FormatNone     Format = "none"
)

// Formats lists every supported format, in help-text order.
func Formats() []Format {
	return []Format{
		FormatTable, FormatJSON, FormatJSONL, FormatCSV,
		FormatTSV, FormatVertical, FormatMarkdown, FormatRaw, FormatNone,
	}
}

// FormatNames returns Formats() as strings.
func FormatNames() []string {
	out := make([]string, 0, len(Formats()))
	for _, f := range Formats() {
		out = append(out, string(f))
	}
	return out
}

// ParseFormat validates a format name, accepting a few aliases.
func ParseFormat(s string) (Format, error) {
	switch strings.ToLower(strings.TrimSpace(s)) {
	case "", "table":
		return FormatTable, nil
	case "json":
		return FormatJSON, nil
	case "jsonl", "ndjson":
		return FormatJSONL, nil
	case "csv":
		return FormatCSV, nil
	case "tsv":
		return FormatTSV, nil
	case "vertical", "v", "expanded":
		return FormatVertical, nil
	case "markdown", "md":
		return FormatMarkdown, nil
	case "raw", "plain":
		return FormatRaw, nil
	case "none", "quiet", "silent":
		return FormatNone, nil
	default:
		return "", fmt.Errorf("unknown format %q (expected one of: %s)", s, strings.Join(FormatNames(), ", "))
	}
}

// Structured reports whether the format is machine-readable, in which case
// status chatter must stay off stdout.
func (f Format) Structured() bool {
	return f == FormatJSON || f == FormatJSONL || f == FormatCSV || f == FormatTSV
}

// Options controls rendering.
type Options struct {
	Format   Format
	MaxWidth int  // per-column cap for table output; 0 uses a default
	MaxRows  int  // stop after n rows; 0 means unlimited
	NoHeader bool // omit the header row where the format has one
	NoFooter bool // omit the trailing row-count line
	Pretty   bool // indent JSON
	NullText string
}

func (o Options) nullText() string {
	if o.NullText == "" {
		return "NULL"
	}
	return o.NullText
}
