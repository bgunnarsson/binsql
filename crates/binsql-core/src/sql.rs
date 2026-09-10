//! Dialect-aware SQL text handling: splitting a script into statements,
//! classifying what each one does, and looking for a keyword in the parts of a
//! statement that will actually execute.
//!
//! This is a lexer, not a parser. It understands just enough syntax — string
//! literals, quoted identifiers and comments — that a semicolon inside a string
//! never splits a statement and a `DELETE` inside a comment never looks like a
//! write. Everything above it, the read-only guard most of all, is only as
//! trustworthy as that distinction.

use crate::backend::Backend;
use crate::error::{Error, Result};
use crate::value::Value;

/// What a statement does to the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `SELECT`, `SHOW`, `EXPLAIN` — reads and nothing else.
    Read,
    /// `INSERT`, `UPDATE`, `DELETE`, `MERGE` — changes data.
    Write,
    /// `CREATE`, `ALTER`, `DROP`, `TRUNCATE` — changes schema.
    Ddl,
    /// `BEGIN`, `COMMIT`, `SET`, `USE` — changes the session, not the database.
    Control,
    /// A statement this lexer does not recognise.
    Unknown,
}

impl Kind {
    /// Whether a statement of this kind can change data or schema.
    ///
    /// `Unknown` counts as mutating. A guard protecting a production database
    /// is the wrong place to give an unrecognised statement the benefit of the
    /// doubt.
    pub fn mutates(self) -> bool {
        matches!(self, Kind::Write | Kind::Ddl | Kind::Unknown)
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Read => "read",
            Kind::Write => "write",
            Kind::Ddl => "ddl",
            Kind::Control => "control",
            Kind::Unknown => "unknown",
        }
    }
}

/// One statement carved out of a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    pub sql: String,
    pub kind: Kind,
}

/// A statement as it goes to the server: its SQL, with any placeholders in the
/// backend's own spelling, and the values that fill them, in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Bound {
    pub sql: String,
    pub params: Vec<Value>,
}

impl Bound {
    /// A statement with nothing to bind, sent exactly as written.
    pub fn plain(sql: impl Into<String>) -> Bound {
        Bound {
            sql: sql.into(),
            params: Vec::new(),
        }
    }
}

/// Breaks a script into statements on top-level semicolons, dropping the empty
/// and comment-only chunks. SQL Server `GO` batch separators split too.
pub fn split(script: &str, backend: Backend) -> Vec<Statement> {
    let mut out: Vec<Statement> = Vec::new();
    let mut current = String::new();

    {
        let mut flush = |current: &mut String| {
            let sql = current.trim().to_string();
            current.clear();
            if sql.is_empty() || !has_code(&sql, backend) {
                return;
            }
            let kind = classify(&sql, backend);
            out.push(Statement { sql, kind });
        };

        scan(script, backend, &mut |token| match token {
            Token::Char(Class::Code, ';') | Token::BatchSeparator => flush(&mut current),
            Token::Char(_, ch) => current.push(ch),
        });
        flush(&mut current);
    }

    out
}

/// Hands each statement its share of `params`, left to right, and rewrites its
/// `?` placeholders into the spelling its backend expects — `$1` for Postgres,
/// `@P1` for SQL Server, and `?` as it stands for SQLite and MySQL. The values
/// travel beside the SQL, never inside it.
///
/// A `?` inside a string, a quoted identifier or a comment is text, not a
/// placeholder. With no values at all nothing is rewritten, so a Postgres `?`
/// operator in a statement that binds nothing reaches the server intact.
///
/// Placeholders and values that do not come out even are refused here, before
/// any statement is sent: a batch that found it had no value for its second
/// statement after running its first would already have done something.
pub fn bind(statements: &[Statement], params: &[Value], backend: Backend) -> Result<Vec<Bound>> {
    if params.is_empty() {
        return Ok(statements
            .iter()
            .map(|statement| Bound::plain(statement.sql.clone()))
            .collect());
    }

    let mut remaining = params;
    let mut found = 0;
    let mut out = Vec::with_capacity(statements.len());
    for statement in statements {
        let (sql, count) = placeholders(&statement.sql, backend);
        found += count;
        let (mine, rest) = remaining.split_at(count.min(remaining.len()));
        remaining = rest;
        out.push(Bound {
            sql,
            params: mine.to_vec(),
        });
    }

    if found != params.len() {
        return Err(Error::Placeholders {
            found,
            given: params.len(),
        });
    }
    Ok(out)
}

/// The statement with each executable `?` rewritten for `backend`, and how
/// many there were. Numbering starts at one per statement, because each
/// statement is sent on its own.
fn placeholders(sql: &str, backend: Backend) -> (String, usize) {
    let mut out = String::with_capacity(sql.len());
    let mut count = 0;
    scan(sql, backend, &mut |token| match token {
        Token::Char(Class::Code, '?') => {
            count += 1;
            match backend {
                Backend::Postgres => out.push_str(&format!("${count}")),
                Backend::MsSql => out.push_str(&format!("@P{count}")),
                Backend::Sqlite | Backend::MySql => out.push('?'),
            }
        }
        Token::Char(_, ch) => out.push(ch),
        // `split` has already cut the script on these, so a statement it
        // produced never holds one.
        Token::BatchSeparator => {}
    });
    (out, count)
}

/// What a single statement does, from its leading keyword.
///
/// `WITH` is looked at further: a CTE fronts an `INSERT`, `UPDATE` or `DELETE`
/// as readily as a `SELECT`, and reading only the first word is how a write
/// gets past a read-only guard.
pub fn classify(sql: &str, backend: Backend) -> Kind {
    let leading = words(sql, backend, 1);
    let Some(first) = leading.first() else {
        return Kind::Unknown;
    };

    match first.as_str() {
        "SELECT" | "SHOW" | "EXPLAIN" | "DESCRIBE" | "DESC" | "VALUES" | "TABLE" => Kind::Read,

        "WITH" => {
            let writes = ["INSERT", "UPDATE", "DELETE", "MERGE", "REPLACE"];
            if words(sql, backend, 0)
                .iter()
                .any(|w| writes.contains(&w.as_str()))
            {
                Kind::Write
            } else {
                Kind::Read
            }
        }

        "INSERT" | "UPDATE" | "DELETE" | "MERGE" | "REPLACE" | "UPSERT" | "COPY" | "LOAD"
        | "CALL" | "EXEC" | "EXECUTE" => Kind::Write,

        // ANALYZE writes statistics back, so it is not a read however much it
        // reads like one.
        "CREATE" | "ALTER" | "DROP" | "TRUNCATE" | "RENAME" | "GRANT" | "REVOKE" | "COMMENT"
        | "VACUUM" | "ANALYZE" | "REINDEX" | "ATTACH" | "DETACH" | "CLUSTER" | "REFRESH" => {
            Kind::Ddl
        }

        "BEGIN" | "START" | "COMMIT" | "ROLLBACK" | "SAVEPOINT" | "RELEASE" | "USE" | "SET" => {
            Kind::Control
        }

        // `PRAGMA x = y` sets something; `PRAGMA table_info(t)` asks something.
        "PRAGMA" => {
            if sql.contains('=') {
                Kind::Ddl
            } else {
                Kind::Read
            }
        }

        _ => Kind::Unknown,
    }
}

/// Whether a bare keyword appears in the executable text of a statement — not
/// inside a string, an identifier or a comment. `keyword` must be upper case.
pub fn has_keyword(sql: &str, backend: Backend, keyword: &str) -> bool {
    words(sql, backend, 0).iter().any(|w| w == keyword)
}

/// The statement with its comments removed, whitespace collapsed, and cut to
/// `max` characters. What to put in a message that names a statement back to
/// someone — an error, a refusal, a log line. `max` of 0 does not truncate.
pub fn summarize(sql: &str, backend: Backend, max: usize) -> String {
    let stripped = strip_comments(sql, backend);
    let collapsed = stripped.split_whitespace().collect::<Vec<_>>().join(" ");

    if max == 0 || collapsed.chars().count() <= max {
        return collapsed;
    }
    let head: String = collapsed.chars().take(max.saturating_sub(1)).collect();
    format!("{head}…")
}

/// The statement without its comments.
pub fn strip_comments(sql: &str, backend: Backend) -> String {
    let mut out = String::new();
    scan(sql, backend, &mut |token| {
        if let Token::Char(class, ch) = token
            && class != Class::Comment
        {
            out.push(ch);
        }
    });
    out.trim().to_string()
}

/// Whether a chunk carries anything executable, as opposed to being only
/// comments and whitespace.
fn has_code(sql: &str, backend: Backend) -> bool {
    let mut found = false;
    scan(sql, backend, &mut |token| match token {
        Token::Char(Class::Code, ch) if !ch.is_whitespace() => found = true,
        Token::Char(Class::Literal, _) => found = true,
        _ => {}
    });
    found
}

/// Upper-cased bare words from the executable text of a statement. `limit` of 0
/// means all of them; anything else stops early, which is all `classify` needs
/// for most statements.
fn words(sql: &str, backend: Backend, limit: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut more = true;

    {
        let push = |word: &mut String, out: &mut Vec<String>| {
            if word.is_empty() {
                return true;
            }
            out.push(word.to_uppercase());
            word.clear();
            limit == 0 || out.len() < limit
        };

        scan(sql, backend, &mut |token| {
            if !more {
                return;
            }
            match token {
                Token::Char(Class::Code, ch) if ch.is_alphabetic() || ch == '_' => word.push(ch),
                _ => more = push(&mut word, &mut out),
            }
        });
        if more {
            push(&mut word, &mut out);
        }
    }

    out
}

// --- lexer ---------------------------------------------------------------

/// The lexical context a character appeared in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    /// Executable SQL, outside any literal or comment.
    Code,
    /// Inside a string literal or a quoted identifier.
    Literal,
    /// Inside a line or block comment.
    Comment,
}

enum Token {
    Char(Class, char),
    /// A SQL Server `GO` line, which ends a batch without being part of one.
    BatchSeparator,
}

/// Walks the SQL emitting one token per character, tagged with the context it
/// appeared in. Everything else in this module is built on it.
fn scan(sql: &str, backend: Backend, emit: &mut impl FnMut(Token)) {
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    let mut at_line_start = true;

    while i < chars.len() {
        let ch = chars[i];

        // Line comment: `-- …`, or `# …` on MySQL.
        let line_comment = (ch == '-' && chars.get(i + 1) == Some(&'-'))
            || (ch == '#' && backend == Backend::MySql);
        if line_comment {
            while i < chars.len() && chars[i] != '\n' {
                emit(Token::Char(Class::Comment, chars[i]));
                i += 1;
            }
            continue;
        }

        // Block comment: `/* … */`.
        if ch == '/' && chars.get(i + 1) == Some(&'*') {
            emit(Token::Char(Class::Comment, chars[i]));
            emit(Token::Char(Class::Comment, chars[i + 1]));
            i += 2;
            while i < chars.len() {
                if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    emit(Token::Char(Class::Comment, chars[i]));
                    emit(Token::Char(Class::Comment, chars[i + 1]));
                    i += 2;
                    break;
                }
                emit(Token::Char(Class::Comment, chars[i]));
                i += 1;
            }
            continue;
        }

        // A single-quoted string, and a double-quoted identifier — which is a
        // string in MySQL's default mode, a distinction that does not matter
        // here because both are opaque.
        if ch == '\'' || ch == '"' {
            i = scan_quoted(&chars, i, ch, backend == Backend::MySql, emit);
            continue;
        }
        if ch == '`' && backend == Backend::MySql {
            i = scan_quoted(&chars, i, '`', false, emit);
            continue;
        }

        // Bracketed identifier, closing bracket doubled to escape itself.
        if ch == '[' && backend == Backend::MsSql {
            emit(Token::Char(Class::Literal, chars[i]));
            i += 1;
            while i < chars.len() {
                if chars[i] == ']' {
                    if chars.get(i + 1) == Some(&']') {
                        emit(Token::Char(Class::Literal, chars[i]));
                        emit(Token::Char(Class::Literal, chars[i + 1]));
                        i += 2;
                        continue;
                    }
                    emit(Token::Char(Class::Literal, chars[i]));
                    i += 1;
                    break;
                }
                emit(Token::Char(Class::Literal, chars[i]));
                i += 1;
            }
            continue;
        }

        // Postgres dollar-quoted body: `$tag$ … $tag$`, which is how a function
        // body full of semicolons arrives.
        if ch == '$'
            && backend == Backend::Postgres
            && let Some(tag) = dollar_tag(&chars, i)
        {
            let stop = match find(&chars, &tag, i + tag.len()) {
                Some(end) => end + tag.len(),
                None => chars.len(),
            };
            while i < stop {
                emit(Token::Char(Class::Literal, chars[i]));
                i += 1;
            }
            continue;
        }

        // `GO` alone on a line ends the batch.
        if backend == Backend::MsSql
            && at_line_start
            && (ch == 'g' || ch == 'G')
            && let Some(next) = go_batch_end(&chars, i)
        {
            emit(Token::BatchSeparator);
            i = next;
            at_line_start = true;
            continue;
        }

        emit(Token::Char(Class::Code, ch));
        at_line_start = ch == '\n' || (at_line_start && ch.is_whitespace());
        i += 1;
    }
}

/// Consumes a quoted run starting at `i` and returns the index just past it. A
/// doubled quote escapes itself everywhere; backslash escapes are MySQL's.
fn scan_quoted(
    chars: &[char],
    mut i: usize,
    quote: char,
    backslash: bool,
    emit: &mut impl FnMut(Token),
) -> usize {
    emit(Token::Char(Class::Literal, chars[i]));
    i += 1;

    while i < chars.len() {
        let ch = chars[i];
        if backslash && ch == '\\' && i + 1 < chars.len() {
            emit(Token::Char(Class::Literal, chars[i]));
            emit(Token::Char(Class::Literal, chars[i + 1]));
            i += 2;
            continue;
        }
        if ch == quote {
            if chars.get(i + 1) == Some(&quote) {
                emit(Token::Char(Class::Literal, chars[i]));
                emit(Token::Char(Class::Literal, chars[i + 1]));
                i += 2;
                continue;
            }
            emit(Token::Char(Class::Literal, chars[i]));
            return i + 1;
        }
        emit(Token::Char(Class::Literal, chars[i]));
        i += 1;
    }

    i
}

/// The full `$tag$` opener at `i`, if there is one.
fn dollar_tag(chars: &[char], i: usize) -> Option<Vec<char>> {
    let mut j = i + 1;
    while let Some(&ch) = chars.get(j) {
        if ch == '_' || ch.is_alphabetic() || (j > i + 1 && ch.is_numeric()) {
            j += 1;
        } else {
            break;
        }
    }
    (chars.get(j) == Some(&'$')).then(|| chars[i..=j].to_vec())
}

fn find(haystack: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.is_empty() || from > haystack.len() {
        return None;
    }
    (from..=haystack.len().saturating_sub(needle.len()))
        .find(|&i| haystack[i..i + needle.len()] == *needle)
}

/// The index just past a `GO` line starting at `i`, if that is what this is.
fn go_batch_end(chars: &[char], i: usize) -> Option<usize> {
    if !matches!(chars.get(i + 1), Some('o' | 'O')) {
        return None;
    }
    let mut j = i + 2;
    while let Some(&ch) = chars.get(j) {
        if ch == '\n' {
            break;
        }
        if !ch.is_whitespace() {
            return None;
        }
        j += 1;
    }
    // Consume the newline, so the next batch starts at the start of a line.
    Some(if j < chars.len() { j + 1 } else { j })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sqls(script: &str, backend: Backend) -> Vec<String> {
        split(script, backend)
            .into_iter()
            .map(|statement| statement.sql)
            .collect()
    }

    #[test]
    fn splits_on_top_level_semicolons() {
        assert_eq!(
            sqls("select 1; select 2;", Backend::Sqlite),
            ["select 1", "select 2"]
        );
        assert_eq!(
            sqls("select 1;\nselect 2", Backend::Sqlite),
            ["select 1", "select 2"]
        );
        assert_eq!(sqls(";;; select 1 ;;;", Backend::Sqlite), ["select 1"]);
    }

    #[test]
    fn a_semicolon_inside_a_literal_does_not_split() {
        assert_eq!(
            sqls(
                "insert into t (s) values ('a;b'); select 1",
                Backend::Sqlite
            ),
            ["insert into t (s) values ('a;b')", "select 1"]
        );
        assert_eq!(
            sqls("select 'it''s here; really'; select 2", Backend::Sqlite),
            ["select 'it''s here; really'", "select 2"]
        );
        assert_eq!(
            sqls("select \"a;b\" from t; select 2", Backend::Postgres),
            ["select \"a;b\" from t", "select 2"]
        );
        assert_eq!(
            sqls("select `a;b` from t; select 2", Backend::MySql),
            ["select `a;b` from t", "select 2"]
        );
        assert_eq!(
            sqls("select [a;b] from t; select 2", Backend::MsSql),
            ["select [a;b] from t", "select 2"]
        );
    }

    #[test]
    fn a_semicolon_inside_a_comment_does_not_split() {
        assert_eq!(
            sqls("select 1 -- ; not a split\n; select 2", Backend::Sqlite),
            ["select 1 -- ; not a split", "select 2"]
        );
        assert_eq!(
            sqls("select 1 /* ; nope */; select 2", Backend::Sqlite),
            ["select 1 /* ; nope */", "select 2"]
        );
    }

    #[test]
    fn comment_only_chunks_are_dropped() {
        assert_eq!(
            sqls("-- just a comment\n; select 1;", Backend::Sqlite),
            ["select 1"]
        );
        assert!(sqls("/* nothing to run */", Backend::Sqlite).is_empty());
    }

    #[test]
    fn mysql_backslash_escapes_a_quote() {
        assert_eq!(
            sqls(r"insert into t values ('a\'; b'); select 2", Backend::MySql),
            [r"insert into t values ('a\'; b')", "select 2"]
        );
    }

    #[test]
    fn sql_server_go_separates_batches() {
        assert_eq!(
            sqls("create table t (id int)\nGO\nselect 1\n", Backend::MsSql),
            ["create table t (id int)", "select 1"]
        );
    }

    #[test]
    fn postgres_dollar_quoted_bodies_stay_whole() {
        assert_eq!(
            sqls(
                "create function f() returns int as $$ begin; return 1; end; $$ language plpgsql; select 2",
                Backend::Postgres
            ),
            [
                "create function f() returns int as $$ begin; return 1; end; $$ language plpgsql",
                "select 2"
            ]
        );
        assert_eq!(
            sqls("select $tag$ a; b $tag$; select 2", Backend::Postgres),
            ["select $tag$ a; b $tag$", "select 2"]
        );
    }

    #[test]
    fn classifies_by_leading_keyword() {
        let cases = [
            ("select * from t", Kind::Read),
            ("  \n SELECT 1", Kind::Read),
            ("explain select 1", Kind::Read),
            ("show tables", Kind::Read),
            ("insert into t values (1)", Kind::Write),
            ("UPDATE t SET a = 1", Kind::Write),
            ("delete from t", Kind::Write),
            ("merge into t using s on (1=1)", Kind::Write),
            ("create table t (id int)", Kind::Ddl),
            ("DROP TABLE t", Kind::Ddl),
            ("truncate table t", Kind::Ddl),
            ("alter table t add column c int", Kind::Ddl),
            ("begin", Kind::Control),
            ("commit", Kind::Control),
            ("set search_path to public", Kind::Control),
            ("pragma table_info(t)", Kind::Read),
            ("pragma foreign_keys = on", Kind::Ddl),
            ("select 'insert into t'", Kind::Read),
        ];
        for (sql, want) in cases {
            assert_eq!(classify(sql, Backend::Sqlite), want, "{sql}");
        }
    }

    #[test]
    fn a_leading_comment_does_not_hide_the_keyword() {
        assert_eq!(
            classify("/* ticket-421 */ DELETE FROM t", Backend::Sqlite),
            Kind::Write
        );
        assert_eq!(
            classify("-- clean up\nDELETE FROM t", Backend::Sqlite),
            Kind::Write
        );
        assert_eq!(
            classify("\n\n  -- x\n  drop table t", Backend::Sqlite),
            Kind::Ddl
        );
    }

    #[test]
    fn a_cte_is_classified_by_what_it_feeds() {
        assert_eq!(
            classify("with x as (select 1) select * from x", Backend::Postgres),
            Kind::Read
        );
        assert_eq!(
            classify(
                "WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x",
                Backend::Postgres
            ),
            Kind::Write
        );
        assert_eq!(
            classify(
                "with x as (delete from t returning *) select * from x",
                Backend::Postgres
            ),
            Kind::Write
        );
        // A column whose name merely starts with a keyword is not one.
        assert_eq!(
            classify(
                "with x as (select insert_date from t) select * from x",
                Backend::Postgres
            ),
            Kind::Read
        );
    }

    #[test]
    fn unknown_statements_count_as_mutating() {
        assert_eq!(classify("frobnicate t", Backend::Sqlite), Kind::Unknown);
        assert!(Kind::Unknown.mutates());
        assert!(Kind::Write.mutates());
        assert!(Kind::Ddl.mutates());
        assert!(!Kind::Read.mutates());
        assert!(!Kind::Control.mutates());
    }

    #[test]
    fn finds_a_keyword_only_where_it_executes() {
        assert!(has_keyword(
            "delete from t where id = 1",
            Backend::Sqlite,
            "WHERE"
        ));
        assert!(!has_keyword("delete from t", Backend::Sqlite, "WHERE"));
        assert!(!has_keyword(
            "delete from t -- where id = 1",
            Backend::Sqlite,
            "WHERE"
        ));
        assert!(!has_keyword(
            "update t set note = 'where clause'",
            Backend::Sqlite,
            "WHERE"
        ));
        assert!(has_keyword(
            "delete from t where s = 'where'",
            Backend::Sqlite,
            "WHERE"
        ));
    }

    #[test]
    fn summarizes_to_one_line() {
        assert_eq!(
            summarize("select   1,\n  2 -- trailing\n", Backend::Sqlite, 0),
            "select 1, 2"
        );
        assert_eq!(
            summarize("select a_very_long_column_name from t", Backend::Sqlite, 10),
            "select a_…"
        );
    }

    fn bound(sql: &str, params: &[Value], backend: Backend) -> Result<Vec<Bound>> {
        bind(&split(sql, backend), params, backend)
    }

    #[test]
    fn placeholders_take_each_backends_spelling() {
        let params = [Value::Int(1), Value::Text("a".into())];
        for (backend, want) in [
            (Backend::Sqlite, "select * from t where a = ? and b = ?"),
            (Backend::MySql, "select * from t where a = ? and b = ?"),
            (Backend::Postgres, "select * from t where a = $1 and b = $2"),
            (Backend::MsSql, "select * from t where a = @P1 and b = @P2"),
        ] {
            let bound = bound("select * from t where a = ? and b = ?", &params, backend).unwrap();
            assert_eq!(bound[0].sql, want, "{backend}");
            assert_eq!(bound[0].params, params);
        }
    }

    #[test]
    fn a_question_mark_in_a_literal_or_a_comment_is_not_a_placeholder() {
        let bound = bound(
            "select '?', \"?\" from t -- why?\nwhere a = ? /* or? */",
            &[Value::Int(1)],
            Backend::Postgres,
        )
        .unwrap();
        assert_eq!(
            bound[0].sql,
            "select '?', \"?\" from t -- why?\nwhere a = $1 /* or? */"
        );
    }

    #[test]
    fn values_fill_a_batch_left_to_right() {
        let params = [Value::Int(1), Value::Int(2), Value::Int(3)];
        let bound = bound(
            "insert into t values (?); update t set a = ? where b = ?",
            &params,
            Backend::Postgres,
        )
        .unwrap();
        assert_eq!(bound[0].sql, "insert into t values ($1)");
        assert_eq!(bound[0].params, [Value::Int(1)]);
        assert_eq!(bound[1].sql, "update t set a = $1 where b = $2");
        assert_eq!(bound[1].params, [Value::Int(2), Value::Int(3)]);
    }

    #[test]
    fn placeholders_and_values_have_to_come_out_even() {
        assert!(matches!(
            bound("select ? + ?", &[Value::Int(1)], Backend::Sqlite),
            Err(Error::Placeholders { found: 2, given: 1 })
        ));
        assert!(matches!(
            bound("select ?", &[Value::Int(1), Value::Int(2)], Backend::Sqlite),
            Err(Error::Placeholders { found: 1, given: 2 })
        ));
    }

    #[test]
    fn nothing_is_rewritten_when_nothing_is_bound() {
        // Postgres's `?` operator asks whether a jsonb value has a key.
        let bound = bound("select data ? 'key' from t", &[], Backend::Postgres).unwrap();
        assert_eq!(bound[0].sql, "select data ? 'key' from t");
        assert!(bound[0].params.is_empty());
    }
}
