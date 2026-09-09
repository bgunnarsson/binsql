package sqlutil

import (
	"reflect"
	"testing"
)

func TestSplit(t *testing.T) {
	tests := []struct {
		name string
		sql  string
		opts Options
		want []string
	}{
		{
			name: "simple pair",
			sql:  "select 1; select 2;",
			want: []string{"select 1", "select 2"},
		},
		{
			name: "trailing statement without semicolon",
			sql:  "select 1;\nselect 2",
			want: []string{"select 1", "select 2"},
		},
		{
			name: "semicolon inside a string literal",
			sql:  "insert into t (s) values ('a;b'); select 1",
			want: []string{"insert into t (s) values ('a;b')", "select 1"},
		},
		{
			name: "escaped quote inside a literal",
			sql:  "select 'it''s here; really'; select 2",
			want: []string{"select 'it''s here; really'", "select 2"},
		},
		{
			name: "semicolon inside a line comment",
			sql:  "select 1 -- ; not a split\n; select 2",
			want: []string{"select 1 -- ; not a split", "select 2"},
		},
		{
			name: "semicolon inside a block comment",
			sql:  "select 1 /* ; nope */; select 2",
			want: []string{"select 1 /* ; nope */", "select 2"},
		},
		{
			name: "comment-only chunks are dropped",
			sql:  "-- just a comment\n; select 1;",
			want: []string{"select 1"},
		},
		{
			name: "empty statements are dropped",
			sql:  ";;; select 1 ;;;",
			want: []string{"select 1"},
		},
		{
			name: "quoted identifier",
			sql:  `select "a;b" from t; select 2`,
			want: []string{`select "a;b" from t`, "select 2"},
		},
		{
			name: "mysql backtick identifier",
			sql:  "select `a;b` from t; select 2",
			opts: Options{Dialect: "mysql"},
			want: []string{"select `a;b` from t", "select 2"},
		},
		{
			name: "mysql backslash escape",
			sql:  `insert into t values ('a\'; b'); select 2`,
			opts: Options{Dialect: "mysql"},
			want: []string{`insert into t values ('a\'; b')`, "select 2"},
		},
		{
			name: "mssql bracket identifier",
			sql:  "select [a;b] from t; select 2",
			opts: Options{Dialect: "mssql"},
			want: []string{"select [a;b] from t", "select 2"},
		},
		{
			name: "mssql GO batch separator",
			sql:  "create table t (id int)\nGO\nselect 1\n",
			opts: Options{Dialect: "mssql"},
			want: []string{"create table t (id int)", "select 1"},
		},
		{
			name: "postgres dollar quoted body",
			sql:  "create function f() returns int as $$ begin; return 1; end; $$ language plpgsql; select 2",
			opts: Options{Dialect: "postgres"},
			want: []string{
				"create function f() returns int as $$ begin; return 1; end; $$ language plpgsql",
				"select 2",
			},
		},
		{
			name: "postgres tagged dollar quote",
			sql:  "select $tag$ a; b $tag$; select 2",
			opts: Options{Dialect: "postgres"},
			want: []string{"select $tag$ a; b $tag$", "select 2"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := Split(tt.sql, tt.opts)
			var sqls []string
			for _, s := range got {
				sqls = append(sqls, s.SQL)
			}
			if !reflect.DeepEqual(sqls, tt.want) {
				t.Errorf("Split() = %#v, want %#v", sqls, tt.want)
			}
		})
	}
}

func TestClassify(t *testing.T) {
	tests := []struct {
		sql  string
		want Kind
	}{
		{"select * from t", KindRead},
		{"  \n SELECT 1", KindRead},
		{"-- comment\nselect 1", KindRead},
		{"/* c */ select 1", KindRead},
		{"with x as (select 1) select * from x", KindRead},
		{"WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x", KindWrite},
		{"with x as (delete from t returning *) select * from x", KindWrite},
		{"insert into t values (1)", KindWrite},
		{"UPDATE t SET a = 1", KindWrite},
		{"delete from t", KindWrite},
		{"merge into t using s on (1=1)", KindWrite},
		{"create table t (id int)", KindDDL},
		{"DROP TABLE t", KindDDL},
		{"truncate table t", KindDDL},
		{"alter table t add column c int", KindDDL},
		{"begin", KindControl},
		{"commit", KindControl},
		{"set search_path to public", KindControl},
		{"explain select 1", KindRead},
		{"show tables", KindRead},
		{"pragma table_info(t)", KindRead},
		{"pragma foreign_keys = on", KindDDL},
		{"select 'insert into t'", KindRead},
	}

	for _, tt := range tests {
		t.Run(tt.sql, func(t *testing.T) {
			if got := Classify(tt.sql, Options{}); got != tt.want {
				t.Errorf("Classify(%q) = %q, want %q", tt.sql, got, tt.want)
			}
		})
	}
}

func TestKindMutates(t *testing.T) {
	mutating := []Kind{KindWrite, KindDDL, KindUnknown}
	for _, k := range mutating {
		if !k.Mutates() {
			t.Errorf("%q should be treated as mutating", k)
		}
	}
	for _, k := range []Kind{KindRead, KindControl} {
		if k.Mutates() {
			t.Errorf("%q should not be treated as mutating", k)
		}
	}
}

func TestHasWhere(t *testing.T) {
	tests := []struct {
		sql  string
		want bool
	}{
		{"delete from t", false},
		{"delete from t where id = 1", true},
		{"update t set a = 1", false},
		{"update t set a = 1 WHERE id = 2", true},
		{"delete from t -- where id = 1", false},
		{"delete from t where s = 'where'", true},
		{"update t set note = 'where clause'", false},
	}

	for _, tt := range tests {
		t.Run(tt.sql, func(t *testing.T) {
			if got := HasWhere(tt.sql, Options{}); got != tt.want {
				t.Errorf("HasWhere(%q) = %v, want %v", tt.sql, got, tt.want)
			}
		})
	}
}

func TestRewrite(t *testing.T) {
	pg := func(n int) string { return "$" + string(rune('0'+n)) }

	tests := []struct {
		name  string
		sql   string
		opts  Options
		want  string
		count int
	}{
		{
			name:  "plain placeholders",
			sql:   "select * from t where a = ? and b = ?",
			want:  "select * from t where a = $1 and b = $2",
			count: 2,
		},
		{
			name:  "question mark inside a literal is left alone",
			sql:   "select * from t where s = 'why?' and a = ?",
			want:  "select * from t where s = 'why?' and a = $1",
			count: 1,
		},
		{
			name:  "question mark inside a comment is left alone",
			sql:   "select 1 -- what?\nwhere a = ?",
			want:  "select 1 -- what?\nwhere a = $1",
			count: 1,
		},
		{
			name:  "no placeholders",
			sql:   "select 1",
			want:  "select 1",
			count: 0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, n := Rewrite(tt.sql, pg, tt.opts)
			if got != tt.want || n != tt.count {
				t.Errorf("Rewrite() = (%q, %d), want (%q, %d)", got, n, tt.want, tt.count)
			}
		})
	}
}

func TestRewriteKeepsQuestionMarkForSqlite(t *testing.T) {
	same := func(int) string { return "?" }
	got, n := Rewrite("select * from t where a = ?", same, Options{})
	if got != "select * from t where a = ?" || n != 1 {
		t.Errorf("Rewrite() = (%q, %d)", got, n)
	}
}

func TestSummarize(t *testing.T) {
	got := Summarize("select   1,\n  2 -- trailing\n", Options{}, 0)
	if got != "select 1, 2" {
		t.Errorf("Summarize() = %q", got)
	}

	if got := Summarize("select a_very_long_column_name from t", Options{}, 10); got != "select a_…" {
		t.Errorf("Summarize() truncation = %q", got)
	}
}
