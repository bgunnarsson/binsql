package render

import (
	"encoding/base64"
	"fmt"
	"strconv"
	"strings"
	"time"
	"unicode"
	"unicode/utf8"
)

// Text renders a value for a human-readable format. Control characters are
// escaped so a stray newline cannot break the table layout.
func Text(v any, nullText string) string {
	s, isNull := text(v)
	if isNull {
		return nullText
	}
	return escapeControl(s)
}

// text is the shared value-to-string conversion; it reports NULL separately so
// each format can choose its own null representation.
func text(v any) (string, bool) {
	if v == nil {
		return "", true
	}
	switch t := v.(type) {
	case []byte:
		if s := string(t); printable(s) {
			return s, false
		}
		return fmt.Sprintf("<binary %d bytes>", len(t)), false
	case string:
		return t, false
	case bool:
		return strconv.FormatBool(t), false
	case int64:
		return strconv.FormatInt(t, 10), false
	case float64:
		return strconv.FormatFloat(t, 'f', -1, 64), false
	case time.Time:
		return t.Format(time.RFC3339Nano), false
	default:
		return fmt.Sprint(t), false
	}
}

// jsonValue maps a database value onto something encoding/json can represent
// faithfully. Numbers stay numbers so downstream tooling need not re-parse.
func jsonValue(v any) any {
	if v == nil {
		return nil
	}
	switch t := v.(type) {
	case []byte:
		if s := string(t); printable(s) {
			return s
		}
		return "base64:" + base64.StdEncoding.EncodeToString(t)
	case time.Time:
		return t.Format(time.RFC3339Nano)
	case string, bool, int, int8, int16, int32, int64,
		uint, uint8, uint16, uint32, uint64, float32, float64:
		return t
	default:
		s, _ := text(t)
		return s
	}
}

func printable(s string) bool {
	if !utf8.ValidString(s) {
		return false
	}
	for _, r := range s {
		if r == '\n' || r == '\t' || r == '\r' {
			continue
		}
		if unicode.IsControl(r) {
			return false
		}
	}
	return true
}

// escapeControl makes a value safe to place inside a single table cell.
func escapeControl(s string) string {
	if !strings.ContainsAny(s, "\n\r\t") {
		return s
	}
	r := strings.NewReplacer("\n", `\n`, "\r", `\r`, "\t", `\t`)
	return r.Replace(s)
}
