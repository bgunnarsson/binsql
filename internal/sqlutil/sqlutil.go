// Package sqlutil provides light-weight, dialect-aware SQL text handling:
// splitting a script into statements, classifying each as read or write, and
// rewriting `?` placeholders into the driver's own bind syntax.
//
// It is a lexer, not a parser. It understands just enough syntax — string
// literals, quoted identifiers and comments — to avoid being fooled by a
// semicolon or question mark inside a string.
package sqlutil

import (
	"strings"
	"unicode"
)

// Kind classifies a statement by what it does to the database.
type Kind string

const (
	KindRead    Kind = "read"    // SELECT, SHOW, EXPLAIN, ...
	KindWrite   Kind = "write"   // INSERT, UPDATE, DELETE, MERGE, ...
	KindDDL     Kind = "ddl"     // CREATE, ALTER, DROP, TRUNCATE, ...
	KindControl Kind = "control" // BEGIN, COMMIT, SET, USE, ...
	KindUnknown Kind = "unknown"
)

// Mutates reports whether a statement of this kind can change data or schema.
func (k Kind) Mutates() bool {
	return k == KindWrite || k == KindDDL || k == KindUnknown
}

// Statement is one statement carved out of a script.
type Statement struct {
	SQL  string
	Kind Kind
}

// Options tunes the lexer for a specific dialect. The zero value is a safe
// generic SQL mode.
type Options struct {
	// Dialect is a driver name: "mysql", "postgres", "mssql" or "sqlite".
	Dialect string
}

func (o Options) backslashEscapes() bool { return o.Dialect == "mysql" }
func (o Options) backtickIdents() bool   { return o.Dialect == "mysql" }
func (o Options) bracketIdents() bool    { return o.Dialect == "mssql" }
func (o Options) dollarQuotes() bool     { return o.Dialect == "postgres" }
func (o Options) goBatches() bool        { return o.Dialect == "mssql" }

// Split breaks a script into individual statements on top-level semicolons,
// dropping empties. SQL Server `GO` batch separators are honoured for mssql.
func Split(script string, opts Options) []Statement {
	var (
		out     []Statement
		current strings.Builder
	)

	flush := func() {
		s := strings.TrimSpace(current.String())
		current.Reset()
		if s == "" || onlyComments(s, opts) {
			return
		}
		out = append(out, Statement{SQL: s, Kind: Classify(s, opts)})
	}

	scan(script, opts, func(ev event) {
		if ev.kind == evCode && ev.r == ';' {
			flush()
			return
		}
		if ev.kind == evBatchSep {
			flush()
			return
		}
		current.WriteRune(ev.r)
	})
	flush()

	return out
}

// Classify determines what a single statement does, based on its leading
// keyword. Statements starting with WITH are inspected further, since a CTE
// can front an INSERT/UPDATE/DELETE just as easily as a SELECT.
func Classify(stmt string, opts Options) Kind {
	words := leadingWords(stmt, opts, 6)
	if len(words) == 0 {
		return KindUnknown
	}

	switch words[0] {
	case "SELECT", "SHOW", "EXPLAIN", "DESCRIBE", "DESC", "VALUES", "TABLE", "ANALYZE":
		return KindRead

	case "WITH":
		// A CTE is read-only unless it feeds a data-modifying statement.
		for _, w := range allWords(stmt, opts) {
			switch w {
			case "INSERT", "UPDATE", "DELETE", "MERGE", "REPLACE":
				return KindWrite
			}
		}
		return KindRead

	case "INSERT", "UPDATE", "DELETE", "MERGE", "REPLACE", "UPSERT", "COPY", "LOAD", "CALL", "EXEC", "EXECUTE":
		return KindWrite

	case "CREATE", "ALTER", "DROP", "TRUNCATE", "RENAME", "GRANT", "REVOKE",
		"COMMENT", "VACUUM", "REINDEX", "ATTACH", "DETACH", "CLUSTER", "REFRESH":
		return KindDDL

	case "BEGIN", "START", "COMMIT", "ROLLBACK", "SAVEPOINT", "RELEASE", "USE", "SET":
		return KindControl

	case "PRAGMA":
		// `PRAGMA x = y` writes; `PRAGMA table_info(t)` reads.
		if strings.Contains(stmt, "=") {
			return KindDDL
		}
		return KindRead

	default:
		return KindUnknown
	}
}

// HasWhere reports whether a statement contains a top-level WHERE keyword.
// Used to guard bare UPDATE/DELETE statements.
func HasWhere(stmt string, opts Options) bool {
	for _, w := range allWords(stmt, opts) {
		if w == "WHERE" {
			return true
		}
	}
	return false
}

// Rewrite replaces each top-level `?` placeholder with the driver's bind
// marker and returns the new SQL plus the number of placeholders found.
// Question marks inside literals, identifiers and comments are left alone.
func Rewrite(sql string, placeholder func(n int) string, opts Options) (string, int) {
	if placeholder == nil {
		return sql, CountPlaceholders(sql, opts)
	}

	var b strings.Builder
	n := 0
	scan(sql, opts, func(ev event) {
		if ev.kind == evCode && ev.r == '?' {
			n++
			b.WriteString(placeholder(n))
			return
		}
		b.WriteRune(ev.r)
	})
	return b.String(), n
}

// CountPlaceholders counts top-level `?` markers.
func CountPlaceholders(sql string, opts Options) int {
	n := 0
	scan(sql, opts, func(ev event) {
		if ev.kind == evCode && ev.r == '?' {
			n++
		}
	})
	return n
}

// StripComments removes comments, mainly so short SQL can be logged on one line.
func StripComments(sql string, opts Options) string {
	var b strings.Builder
	scan(sql, opts, func(ev event) {
		if ev.kind == evComment {
			return
		}
		b.WriteRune(ev.r)
	})
	return strings.TrimSpace(b.String())
}

// Summarize renders a statement as a single short line for status messages.
func Summarize(sql string, opts Options, max int) string {
	s := strings.Join(strings.Fields(StripComments(sql, opts)), " ")
	if max > 0 && len([]rune(s)) > max {
		return string([]rune(s)[:max-1]) + "…"
	}
	return s
}

// --- lexer ---------------------------------------------------------------

type evKind int

const (
	evCode     evKind = iota // executable SQL text outside any literal
	evLiteral                // inside a string literal or quoted identifier
	evComment                // inside a line or block comment
	evBatchSep               // a `GO` batch separator (mssql); r is unused
)

type event struct {
	kind evKind
	r    rune
}

// scan walks the SQL emitting one event per rune, tagged with the lexical
// context it appeared in. Everything else in this package is built on it.
func scan(sql string, opts Options, emit func(event)) {
	rs := []rune(sql)
	i := 0
	atLineStart := true

	for i < len(rs) {
		r := rs[i]

		// Line comment: -- ... or # ... (mysql)
		if r == '-' && i+1 < len(rs) && rs[i+1] == '-' {
			for i < len(rs) && rs[i] != '\n' {
				emit(event{evComment, rs[i]})
				i++
			}
			continue
		}
		if r == '#' && opts.Dialect == "mysql" {
			for i < len(rs) && rs[i] != '\n' {
				emit(event{evComment, rs[i]})
				i++
			}
			continue
		}

		// Block comment: /* ... */
		if r == '/' && i+1 < len(rs) && rs[i+1] == '*' {
			emit(event{evComment, rs[i]})
			emit(event{evComment, rs[i+1]})
			i += 2
			for i < len(rs) {
				if rs[i] == '*' && i+1 < len(rs) && rs[i+1] == '/' {
					emit(event{evComment, rs[i]})
					emit(event{evComment, rs[i+1]})
					i += 2
					break
				}
				emit(event{evComment, rs[i]})
				i++
			}
			continue
		}

		// Single-quoted string.
		if r == '\'' {
			i = scanQuoted(rs, i, '\'', opts.backslashEscapes(), emit)
			continue
		}

		// Double-quoted identifier (a string in mysql's default mode, but the
		// distinction does not matter to us — both are opaque).
		if r == '"' {
			i = scanQuoted(rs, i, '"', opts.backslashEscapes(), emit)
			continue
		}

		if r == '`' && opts.backtickIdents() {
			i = scanQuoted(rs, i, '`', false, emit)
			continue
		}

		// Bracketed identifier: [name]], closing bracket doubled to escape.
		if r == '[' && opts.bracketIdents() {
			emit(event{evLiteral, rs[i]})
			i++
			for i < len(rs) {
				if rs[i] == ']' {
					if i+1 < len(rs) && rs[i+1] == ']' {
						emit(event{evLiteral, rs[i]})
						emit(event{evLiteral, rs[i+1]})
						i += 2
						continue
					}
					emit(event{evLiteral, rs[i]})
					i++
					break
				}
				emit(event{evLiteral, rs[i]})
				i++
			}
			continue
		}

		// Postgres dollar-quoted body: $tag$ ... $tag$
		if r == '$' && opts.dollarQuotes() {
			if tag, ok := dollarTag(rs, i); ok {
				end := indexRunes(rs, tag, i+len(tag))
				stop := len(rs)
				if end >= 0 {
					stop = end + len(tag)
				}
				for ; i < stop; i++ {
					emit(event{evLiteral, rs[i]})
				}
				continue
			}
		}

		// SQL Server `GO` on a line of its own ends the batch.
		if opts.goBatches() && atLineStart && (r == 'g' || r == 'G') {
			if n, ok := matchGoBatch(rs, i); ok {
				emit(event{kind: evBatchSep})
				i = n
				atLineStart = true
				continue
			}
		}

		emit(event{evCode, r})
		atLineStart = r == '\n' || (atLineStart && unicode.IsSpace(r))
		i++
	}
}

// scanQuoted consumes a quoted run starting at i and returns the next index.
// A doubled quote escapes itself in every dialect; backslash escapes are
// MySQL-only.
func scanQuoted(rs []rune, i int, quote rune, backslash bool, emit func(event)) int {
	emit(event{evLiteral, rs[i]})
	i++
	for i < len(rs) {
		r := rs[i]
		if backslash && r == '\\' && i+1 < len(rs) {
			emit(event{evLiteral, rs[i]})
			emit(event{evLiteral, rs[i+1]})
			i += 2
			continue
		}
		if r == quote {
			if i+1 < len(rs) && rs[i+1] == quote {
				emit(event{evLiteral, rs[i]})
				emit(event{evLiteral, rs[i+1]})
				i += 2
				continue
			}
			emit(event{evLiteral, rs[i]})
			return i + 1
		}
		emit(event{evLiteral, rs[i]})
		i++
	}
	return i
}

// dollarTag returns the full `$tag$` opener at position i, if there is one.
func dollarTag(rs []rune, i int) ([]rune, bool) {
	j := i + 1
	for j < len(rs) && (rs[j] == '_' || unicode.IsLetter(rs[j]) || (j > i+1 && unicode.IsDigit(rs[j]))) {
		j++
	}
	if j < len(rs) && rs[j] == '$' {
		return rs[i : j+1], true
	}
	return nil, false
}

func indexRunes(hay []rune, needle []rune, from int) int {
	if len(needle) == 0 || from < 0 {
		return -1
	}
	for i := from; i+len(needle) <= len(hay); i++ {
		if string(hay[i:i+len(needle)]) == string(needle) {
			return i
		}
	}
	return -1
}

// matchGoBatch checks for a `GO` line at i and returns the index just past it.
func matchGoBatch(rs []rune, i int) (int, bool) {
	if i+1 >= len(rs) {
		return 0, false
	}
	if (rs[i] != 'g' && rs[i] != 'G') || (rs[i+1] != 'o' && rs[i+1] != 'O') {
		return 0, false
	}
	j := i + 2
	for j < len(rs) && rs[j] != '\n' {
		if !unicode.IsSpace(rs[j]) {
			return 0, false
		}
		j++
	}
	if j < len(rs) {
		j++ // consume the newline
	}
	return j, true
}

// --- word extraction ------------------------------------------------------

// words pulls upper-cased bare words out of the code portions of a statement.
// limit <= 0 means "all of them".
func words(sql string, opts Options, limit int) []string {
	var (
		out  []string
		word strings.Builder
	)

	push := func() bool {
		if word.Len() == 0 {
			return true
		}
		out = append(out, strings.ToUpper(word.String()))
		word.Reset()
		return limit <= 0 || len(out) < limit
	}

	more := true
	scan(sql, opts, func(ev event) {
		if !more {
			return
		}
		if ev.kind != evCode {
			more = push()
			return
		}
		if unicode.IsLetter(ev.r) || ev.r == '_' {
			word.WriteRune(ev.r)
			return
		}
		more = push()
	})
	if more {
		push()
	}
	return out
}

func leadingWords(sql string, opts Options, n int) []string { return words(sql, opts, n) }
func allWords(sql string, opts Options) []string            { return words(sql, opts, 0) }

// onlyComments reports whether a chunk carries no executable text.
func onlyComments(sql string, opts Options) bool {
	empty := true
	scan(sql, opts, func(ev event) {
		if ev.kind == evCode && !unicode.IsSpace(ev.r) {
			empty = false
		}
		if ev.kind == evLiteral {
			empty = false
		}
	})
	return empty
}
