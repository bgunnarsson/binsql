//! Runs the real binary the way a script would, against a real database.
//!
//! Command mode's contract is its output and its exit code, so both are
//! asserted here rather than the functions underneath them: a change that
//! quietly moves a message from stdout to stderr breaks a caller, and only a
//! test at this level notices.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// One directory per test — these run concurrently, and a shared config or
/// database would have them treading on each other.
struct Fixture {
    directory: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let directory = std::env::temp_dir().join(format!(
            "binsql-cli-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create the fixture directory");

        let fixture = Fixture { directory };
        std::fs::write(fixture.database(), b"").expect("create the database file");
        fixture
    }

    fn database(&self) -> PathBuf {
        self.directory.join("test.db")
    }

    fn config(&self) -> PathBuf {
        self.directory.join("connections.json")
    }

    /// Writes a config naming the fixture's database twice: once writable, once
    /// registered read-only.
    fn write_config(&self) {
        let database = self.database();
        let database = database.display();
        std::fs::write(
            self.config(),
            format!(
                r#"{{
                  "default": "local",
                  "connections": {{
                    "local": {{ "driver": "sqlite", "dsn": "{database}" }},
                    "prod": {{ "driver": "sqlite", "dsn": "{database}", "readonly": true }}
                  }}
                }}"#
            ),
        )
        .expect("write the config");
    }

    fn binsql(&self, args: &[&str]) -> Run {
        self.binsql_with_project(args, "")
    }

    /// The same, with `project` as the project config; empty turns it off.
    fn binsql_with_project(&self, args: &[&str], project: &str) -> Run {
        self.binsql_with_env(args, project, &[])
    }

    /// The same, with `variables` set in its environment.
    fn binsql_with_env(&self, args: &[&str], project: &str, variables: &[(&str, &str)]) -> Run {
        let output = Command::new(env!("CARGO_BIN_EXE_binsql"))
            .args(args)
            .env("BINSQL_CONFIG", self.config())
            .env("BINSQL_PROJECT", project)
            // The tests must not read whatever the developer running them has
            // in their own environment.
            .env_remove("BINSQL_CONN")
            .env_remove("BINSQL_DSN")
            .env_remove("BINSQL_DRIVER")
            .env_remove("BINSQL_ERROR_FORMAT")
            .envs(variables.iter().copied())
            .output()
            .expect("run binsql");
        Run::new(output)
    }

    /// The same, with the database named by DSN rather than through the config.
    /// The verb has to stay first, so the flag goes in behind it.
    fn direct(&self, args: &[&str]) -> Run {
        let database = self.database();
        let (verb, rest) = args.split_first().expect("a verb");
        let mut all = vec![*verb, "--dsn", database.to_str().expect("utf-8 path")];
        all.extend_from_slice(rest);
        self.binsql(&all)
    }

    fn seed(&self) {
        self.direct(&[
            "exec",
            "CREATE TABLE artist (id INTEGER PRIMARY KEY, name TEXT NOT NULL, founded INTEGER); \
             INSERT INTO artist (name, founded) \
             VALUES ('Portishead', 1991), ('Boards of Canada', 1986), ('Autechre', NULL)",
        ])
        .succeeds();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Run {
    fn new(output: Output) -> Run {
        Run {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    fn succeeds(self) -> Run {
        assert_eq!(
            self.code, 0,
            "expected success\n{}{}",
            self.stdout, self.stderr
        );
        self
    }

    /// A usage mistake, which the caller is meant to be able to tell from a
    /// database that said no.
    fn refused(self) -> Run {
        assert_eq!(
            self.code, 2,
            "expected exit 2\n{}{}",
            self.stdout, self.stderr
        );
        self
    }

    fn failed(self) -> Run {
        assert_eq!(
            self.code, 1,
            "expected exit 1\n{}{}",
            self.stdout, self.stderr
        );
        self
    }

    fn stdout_has(self, needle: &str) -> Run {
        assert!(
            self.stdout.contains(needle),
            "{needle:?} missing from stdout:\n{}",
            self.stdout
        );
        self
    }

    fn stderr_has(self, needle: &str) -> Run {
        assert!(
            self.stderr.contains(needle),
            "{needle:?} missing from stderr:\n{}",
            self.stderr
        );
        self
    }
}

fn args<'a>(verb: &'a str, rest: &[&'a str]) -> Vec<&'a str> {
    let mut all = vec![verb];
    all.extend_from_slice(rest);
    all
}

#[test]
fn query_prints_a_table_and_every_other_format() {
    let fixture = Fixture::new("formats");
    fixture.seed();

    fixture
        .direct(&args("query", &["SELECT * FROM artist ORDER BY id"]))
        .succeeds()
        .stdout_has("Portishead")
        .stdout_has("NULL")
        .stdout_has("(3 rows");

    fixture
        .direct(&args(
            "query",
            &["SELECT name FROM artist ORDER BY id", "-o", "csv"],
        ))
        .succeeds()
        .stdout_has("name\nPortishead\nBoards of Canada\nAutechre\n");

    let json = fixture
        .direct(&args(
            "query",
            &["SELECT * FROM artist ORDER BY id", "-o", "json"],
        ))
        .succeeds();
    let parsed: serde_json::Value = serde_json::from_str(&json.stdout).expect("valid json");
    assert_eq!(parsed["row_count"], 3);
    assert_eq!(parsed["rows"][0]["name"], "Portishead");
    assert_eq!(parsed["rows"][2]["founded"], serde_json::Value::Null);

    // The structured formats have to be the only thing on stdout, or the first
    // thing a caller parses is a sentence.
    fixture
        .direct(&args(
            "query",
            &["SELECT 1 AS n", "-o", "jsonl", "--no-footer"],
        ))
        .succeeds()
        .stdout_has("{\"n\":1}\n");
}

#[test]
fn query_refuses_to_write_and_refuses_a_script() {
    let fixture = Fixture::new("query-refuses");
    fixture.seed();

    fixture
        .direct(&args("query", &["DELETE FROM artist WHERE id = 1"]))
        .refused()
        .stderr_has("use `binsql exec`");

    // EXPLAIN ANALYZE runs what it explains, so it is the write it wraps.
    fixture
        .direct(&args(
            "query",
            &["EXPLAIN ANALYZE DELETE FROM artist WHERE id = 1"],
        ))
        .refused()
        .stderr_has("use `binsql exec`");

    fixture
        .direct(&args("query", &["SELECT 1; SELECT 2"]))
        .refused()
        .stderr_has("binsql exec");

    // Nothing was written by either refusal.
    fixture
        .direct(&args(
            "query",
            &["SELECT count(*) FROM artist", "-o", "raw"],
        ))
        .succeeds()
        .stdout_has("3");
}

#[test]
fn exec_runs_a_batch_in_one_transaction() {
    let fixture = Fixture::new("exec-batch");
    fixture.seed();

    // The second statement fails, so neither is kept.
    fixture
        .direct(&args(
            "exec",
            &["INSERT INTO artist (name) VALUES ('Aphex Twin'); INSERT INTO nope (x) VALUES (1)"],
        ))
        .failed()
        .stderr_has("rolled back");

    fixture
        .direct(&args(
            "query",
            &["SELECT count(*) FROM artist", "-o", "raw"],
        ))
        .succeeds()
        .stdout_has("3");
}

#[test]
fn a_dry_run_keeps_nothing() {
    let fixture = Fixture::new("dry-run");
    fixture.seed();

    fixture
        .direct(&args(
            "exec",
            &["UPDATE artist SET founded = 0 WHERE id = 1", "--dry-run"],
        ))
        .succeeds()
        .stdout_has("1 row")
        .stdout_has("rolled back");

    fixture
        .direct(&args(
            "query",
            &["SELECT founded FROM artist WHERE id = 1", "-o", "raw"],
        ))
        .succeeds()
        .stdout_has("1991");
}

#[test]
fn exec_needs_force_for_the_irreversible() {
    let fixture = Fixture::new("force");
    fixture.seed();

    fixture
        .direct(&args("exec", &["DELETE FROM artist"]))
        .refused()
        .stderr_has("--force");
    fixture
        .direct(&args("exec", &["DROP TABLE artist"]))
        .refused()
        .stderr_has("--force");

    fixture
        .direct(&args(
            "query",
            &["SELECT count(*) FROM artist", "-o", "raw"],
        ))
        .succeeds()
        .stdout_has("3");

    fixture
        .direct(&args("exec", &["DELETE FROM artist", "--force"]))
        .succeeds();
    fixture
        .direct(&args(
            "query",
            &["SELECT count(*) FROM artist", "-o", "raw"],
        ))
        .succeeds()
        .stdout_has("0");
}

#[test]
fn query_plan_prints_the_estimated_plan_without_running_it() {
    let fixture = Fixture::new("query-plan");
    fixture.write_config();
    fixture.seed();
    let read = "SELECT * FROM artist WHERE name = 'x'";

    let json = fixture
        .direct(&args("query", &["--plan", read, "-o", "json"]))
        .succeeds();
    let parsed: serde_json::Value = serde_json::from_str(&json.stdout).expect("valid json");
    assert_eq!(parsed["rows"][0]["detail"], "SCAN artist");

    fixture
        .direct(&args("query", &["--plan", read, "-o", "csv"]))
        .succeeds()
        .stdout_has("id,parent,notused,detail\n")
        .stdout_has("SCAN artist");

    fixture
        .direct(&args("query", &["--plan", read, "-o", "raw"]))
        .succeeds()
        .stdout_has("\tSCAN artist\n");

    // A read-only source may plan: nothing is run.
    fixture
        .binsql(&["query", "--conn", "prod", "--plan", read])
        .succeeds()
        .stdout_has("SCAN artist");

    for sql in ["EXPLAIN SELECT 1", "PRAGMA table_info(artist)"] {
        fixture
            .direct(&args("query", &["--plan", sql]))
            .refused()
            .stderr_has("--plan plans one SELECT");
    }

    fixture
        .direct(&args(
            "query",
            &[
                "--plan",
                "SELECT * FROM artist WHERE id = ?",
                "--arg",
                "int:1",
            ],
        ))
        .refused()
        .stderr_has("drop --arg");

    fixture
        .direct(&args(
            "query",
            &["--plan", "--allow-write", "DELETE FROM artist WHERE id = 1"],
        ))
        .refused()
        .stderr_has("drop --plan");
    fixture
        .direct(&args(
            "query",
            &["SELECT count(*) FROM artist", "-o", "raw"],
        ))
        .succeeds()
        .stdout_has("3");
}

#[test]
fn a_read_only_data_source_refuses_a_write_from_the_command_line() {
    let fixture = Fixture::new("readonly");
    fixture.write_config();
    fixture.seed();

    fixture
        .binsql(&[
            "exec",
            "--conn",
            "prod",
            "UPDATE artist SET founded = 0 WHERE id = 1",
        ])
        .failed()
        .stderr_has("read-only");

    // The disguises the guard used to fall for, through the real binary.
    for sql in [
        "/* ticket-421 */ DELETE FROM artist WHERE id = 1",
        "SELECT 1; DELETE FROM artist WHERE id = 1",
        "EXPLAIN ANALYZE DELETE FROM artist WHERE id = 1",
    ] {
        fixture
            .binsql(&["exec", "--conn", "prod", sql, "--force"])
            .failed()
            .stderr_has("read-only");
    }

    // Binding values changes what is sent, not what the statement does.
    fixture
        .binsql(&[
            "exec",
            "--conn",
            "prod",
            "UPDATE artist SET founded = ? WHERE id = ?",
            "--arg",
            "int:0",
            "--arg",
            "int:1",
        ])
        .failed()
        .stderr_has("read-only");

    fixture
        .binsql(&[
            "query",
            "--conn",
            "prod",
            "SELECT count(*) FROM artist",
            "-o",
            "raw",
        ])
        .succeeds()
        .stdout_has("3");
}

#[test]
fn values_are_bound_rather_than_spliced_in() {
    let fixture = Fixture::new("bind");
    fixture.seed();

    // Untyped, a value is text, which SQLite compares with an integer column
    // as the number it spells.
    fixture
        .direct(&args(
            "query",
            &[
                "SELECT name FROM artist WHERE id = ?",
                "--arg",
                "2",
                "-o",
                "raw",
            ],
        ))
        .succeeds()
        .stdout_has("Boards of Canada");

    fixture
        .direct(&args(
            "query",
            &[
                "SELECT name FROM artist WHERE founded < ? AND name <> ?",
                "--arg",
                "int:1990",
                "--arg",
                "Autechre",
                "-o",
                "raw",
            ],
        ))
        .succeeds()
        .stdout_has("Boards of Canada");

    // A value that would be SQL if it were pasted in is only ever a value.
    fixture
        .direct(&args(
            "exec",
            &[
                "INSERT INTO artist (name, founded) VALUES (?, ?)",
                "--arg",
                "x'); DROP TABLE artist; --",
                "--arg",
                "null:",
            ],
        ))
        .succeeds()
        .stdout_has("1 row");
    fixture
        .direct(&args(
            "query",
            &[
                "SELECT count(*) FROM artist WHERE founded IS NULL",
                "-o",
                "raw",
            ],
        ))
        .succeeds()
        .stdout_has("2");

    // A question mark in a string is text, and str: keeps a prefix as text.
    fixture
        .direct(&args(
            "query",
            &["SELECT '?' || ? AS s", "--arg", "str:int:7", "-o", "raw"],
        ))
        .succeeds()
        .stdout_has("?int:7");
}

#[test]
fn placeholders_and_values_have_to_match() {
    let fixture = Fixture::new("bind-count");
    fixture.seed();

    fixture
        .direct(&args(
            "query",
            &[
                "SELECT * FROM artist WHERE id = ? AND name = ?",
                "--arg",
                "int:1",
            ],
        ))
        .refused()
        .stderr_has("2 placeholders in the SQL, but 1 value to bind");
    fixture
        .direct(&args("query", &["SELECT ?", "--arg", "int:seven"]))
        .refused()
        .stderr_has("is not an integer");

    // A batch takes its values left to right, and a shortfall is found before
    // any of it runs — without a transaction, the first insert would stay.
    let batch = "INSERT INTO artist (name) VALUES (?); INSERT INTO artist (name) VALUES (?)";
    fixture
        .direct(&args("exec", &[batch, "--arg", "Aphex Twin", "--no-tx"]))
        .refused();
    fixture
        .direct(&args(
            "query",
            &["SELECT count(*) FROM artist", "-o", "raw"],
        ))
        .succeeds()
        .stdout_has("3");

    fixture
        .direct(&args(
            "exec",
            &[batch, "--arg", "Aphex Twin", "--arg", "Plaid", "--no-tx"],
        ))
        .succeeds();
    fixture
        .direct(&args(
            "query",
            &[
                "SELECT name FROM artist WHERE id > 3 ORDER BY id",
                "-o",
                "raw",
            ],
        ))
        .succeeds()
        .stdout_has("Aphex Twin\nPlaid\n");
}

#[test]
fn inspect_lists_tables_and_describes_one() {
    let fixture = Fixture::new("inspect");
    fixture.seed();

    fixture
        .direct(&["inspect"])
        .succeeds()
        .stdout_has("artist")
        .stdout_has("table");

    fixture
        .direct(&["inspect", "artist", "-o", "csv"])
        .succeeds()
        .stdout_has("column,type,nullable,default,primary_key")
        .stdout_has("id,INTEGER,true,,true")
        .stdout_has("name,TEXT,false,,false");

    fixture
        .direct(&["inspect", "nonesuch"])
        .failed()
        .stderr_has("no table or view named nonesuch");
}

#[test]
fn inspect_lists_columns_in_declared_order() {
    let fixture = Fixture::new("inspect-order");
    fixture
        .direct(&[
            "exec",
            "CREATE TABLE track (title TEXT NOT NULL, album INTEGER, id INTEGER PRIMARY KEY)",
        ])
        .succeeds();

    fixture
        .direct(&["inspect", "track", "-o", "csv"])
        .succeeds()
        .stdout_has(
            "column,type,nullable,default,primary_key\n\
             title,TEXT,false,,false\n\
             album,INTEGER,true,,false\n\
             id,INTEGER,true,,true\n",
        );
}

#[test]
fn inspect_columns_lists_every_object_with_its_identity() {
    let fixture = Fixture::new("inspect-columns");
    fixture
        .direct(&[
            "exec",
            "CREATE TABLE track (title TEXT NOT NULL, id INTEGER PRIMARY KEY); \
             CREATE VIEW long_track AS SELECT title FROM track",
        ])
        .succeeds();

    let run = fixture
        .direct(&["inspect", "--columns", "-o", "json"])
        .succeeds();
    let mut parsed: serde_json::Value = serde_json::from_str(&run.stdout).expect("json");
    parsed
        .as_object_mut()
        .expect("an envelope")
        .remove("duration_ms")
        .expect("a duration");
    assert_eq!(
        parsed,
        serde_json::json!({
            "columns": [
                {"name": "catalog", "type": "text"},
                {"name": "schema", "type": "text"},
                {"name": "object", "type": "text"},
                {"name": "kind", "type": "text"},
                {"name": "column", "type": "text"},
                {"name": "type", "type": "text"},
                {"name": "nullable", "type": "bool"},
                {"name": "default", "type": "text"},
                {"name": "primary_key", "type": "bool"},
            ],
            "row_count": 3,
            "rows": [
                {"catalog": "main", "schema": null, "object": "long_track", "kind": "view",
                 "column": "title", "type": "TEXT", "nullable": true, "default": null,
                 "primary_key": false},
                {"catalog": "main", "schema": null, "object": "track", "kind": "table",
                 "column": "title", "type": "TEXT", "nullable": false, "default": null,
                 "primary_key": false},
                {"catalog": "main", "schema": null, "object": "track", "kind": "table",
                 "column": "id", "type": "INTEGER", "nullable": true, "default": null,
                 "primary_key": true},
            ],
            "truncated": false,
        })
    );

    let run = fixture
        .direct(&["inspect", "--columns", "-o", "jsonl"])
        .succeeds();
    let lines: Vec<serde_json::Value> = run
        .stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect("a json line"))
        .collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0]["object"], "long_track");
    assert_eq!(lines[2]["column"], "id");

    let run = fixture
        .direct(&["inspect", "--columns", "-o", "csv"])
        .succeeds();
    assert_eq!(
        run.stdout,
        "catalog,schema,object,kind,column,type,nullable,default,primary_key\n\
         main,,long_track,view,title,TEXT,true,,false\n\
         main,,track,table,title,TEXT,false,,false\n\
         main,,track,table,id,INTEGER,true,,true\n"
    );
}

#[test]
fn inspect_columns_names_one_object_exactly() {
    let fixture = Fixture::new("inspect-columns-name");
    fixture.seed();
    fixture
        .direct(&["exec", "CREATE TABLE album (id INTEGER)"])
        .succeeds();

    let run = fixture
        .direct(&["inspect", "--columns", "artist", "-o", "csv"])
        .succeeds();
    assert_eq!(
        run.stdout,
        "catalog,schema,object,kind,column,type,nullable,default,primary_key\n\
         main,,artist,table,id,INTEGER,true,,true\n\
         main,,artist,table,name,TEXT,false,,false\n\
         main,,artist,table,founded,INTEGER,true,,false\n"
    );

    let run = fixture
        .direct(&["inspect", "--columns", "ARTIST"])
        .failed()
        .stderr_has("no table or view named ARTIST in main")
        .stderr_has("names that differ only in case: artist");
    assert_eq!(run.stdout, "");

    fixture
        .direct(&["inspect", "--columns", "nonesuch"])
        .failed()
        .stderr_has("no table or view named nonesuch");
}

#[test]
fn inspect_columns_names_an_object_with_a_dot() {
    let fixture = Fixture::new("inspect-columns-dot");
    fixture
        .direct(&["exec", "CREATE TABLE \"a.b\" (id INTEGER)"])
        .succeeds();

    let run = fixture
        .direct(&["inspect", "--columns", "a.b", "-o", "csv"])
        .succeeds();
    assert_eq!(
        run.stdout,
        "catalog,schema,object,kind,column,type,nullable,default,primary_key\n\
         main,,a.b,table,id,INTEGER,true,,false\n"
    );

    fixture.direct(&["inspect", "a.b"]).failed();
}

#[test]
fn inspect_without_columns_prints_what_it_did_before() {
    let fixture = Fixture::new("inspect-unchanged");
    fixture.seed();

    assert_eq!(
        fixture.direct(&["inspect", "-o", "csv"]).succeeds().stdout,
        "schema,name,kind\n,artist,table\n"
    );
    assert_eq!(
        fixture
            .direct(&["inspect", "-o", "jsonl"])
            .succeeds()
            .stdout,
        "{\"kind\":\"table\",\"name\":\"artist\",\"schema\":\"\"}\n"
    );
    assert_eq!(
        fixture
            .direct(&["inspect", "ARTIST", "-o", "csv"])
            .succeeds()
            .stdout,
        fixture
            .direct(&["inspect", "artist", "-o", "csv"])
            .succeeds()
            .stdout
    );
    assert_eq!(
        fixture
            .direct(&["inspect", "artist", "-o", "csv"])
            .succeeds()
            .stdout,
        "column,type,nullable,default,primary_key\n\
         id,INTEGER,true,,true\n\
         name,TEXT,false,,false\n\
         founded,INTEGER,true,,false\n"
    );
}

#[test]
fn a_saved_data_source_and_the_default_both_work() {
    let fixture = Fixture::new("saved");
    fixture.write_config();
    fixture.seed();

    fixture
        .binsql(&[
            "query",
            "--conn",
            "local",
            "SELECT count(*) FROM artist",
            "-o",
            "raw",
        ])
        .succeeds()
        .stdout_has("3");

    // No --conn and no --dsn: the config's `default`.
    fixture
        .binsql(&["query", "SELECT count(*) FROM artist", "-o", "raw"])
        .succeeds()
        .stdout_has("3");

    fixture
        .binsql(&["query", "--conn", "nonesuch", "SELECT 1"])
        .refused()
        .stderr_has("no saved data source named nonesuch");
}

#[test]
fn usage_mistakes_exit_two_and_say_what_was_wrong() {
    let fixture = Fixture::new("usage");
    fixture.write_config();

    fixture
        .binsql(&["query", "SELECT 1", "-o", "yaml"])
        .refused()
        .stderr_has("unknown format yaml");
    fixture
        .binsql(&["query", "SELECT 1", "--nope"])
        .refused()
        .stderr_has("unknown option --nope");
    fixture
        .binsql(&["exec", "SELECT 1", "--tx", "--no-tx"])
        .refused()
        .stderr_has("contradict");
    // Not a verb, so it is read as a data source to open — and it is not one
    // of those either.
    fixture
        .binsql(&["frobnicate"])
        .failed()
        .stderr_has("not a saved data source");
}

#[test]
fn source_is_a_command_only_with_an_operation_after_it() {
    let fixture = Fixture::new("source-dispatch");
    fixture.write_config();

    // Alone it is still a data source to open, and there is none named so.
    fixture
        .binsql(&["source"])
        .failed()
        .stderr_has("not a saved data source");
    let help = fixture.binsql(&["source", "--help"]).succeeds();
    assert!(help.stdout.contains("COMMAND MODE"), "{}", help.stdout);

    fixture
        .binsql(&["source", "frob"])
        .refused()
        .stderr_has("unknown source command frob");
}

/// A config with every kind of row `source list` has to get right, and a
/// project config beside it so both scopes show.
fn write_sources(fixture: &Fixture) -> String {
    std::fs::write(
        fixture.config(),
        r#"{
          "default": "inspect",
          "connections": {
            "pg": { "driver": "postgres", "dsn": "postgres://ann:hunter2@db/app", "description": "app" },
            "kc": { "driver": "mssql", "dsn": "keychain://kc", "readonly": true },
            "query": { "driver": "sqlite", "dsn": "/tmp/q.db" },
            "source": { "driver": "sqlite", "dsn": "/tmp/s.db" },
            "team": {
              "inspect": { "driver": "mysql", "dsn": "keyvault://kv-team/dsn", "open_on_start": true }
            }
          }
        }"#,
    )
    .expect("write the config");
    let project = fixture.directory.join(".binsql.json");
    std::fs::write(
        &project,
        r#"{ "connections": { "here": { "driver": "sqlite", "dsn": "/tmp/h.db" } } }"#,
    )
    .expect("write the project config");
    project.display().to_string()
}

#[test]
fn source_list_shows_every_data_source_without_its_secret() {
    let fixture = Fixture::new("source-list");
    let project = write_sources(&fixture);

    let json = fixture
        .binsql_with_project(&["source", "list", "-o", "json"], &project)
        .succeeds();
    assert!(!json.stdout.contains("hunter2"), "{}", json.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&json.stdout).expect("valid json");
    let rows = parsed["rows"].as_array().expect("rows");
    let row = |name: &str| {
        rows.iter()
            .find(|row| row["name"] == name)
            .unwrap_or_else(|| panic!("{name} missing from {}", json.stdout))
            .clone()
    };
    assert_eq!(rows.len(), 6);

    let pg = row("pg");
    assert_eq!(pg["scope"], "user");
    assert_eq!(pg["driver"], "postgres");
    assert!(!pg["dsn"].as_str().unwrap().contains("hunter2"));
    assert!(pg["dsn"].as_str().unwrap().starts_with("postgres://ann:"));
    assert_eq!(pg["readonly"], false);
    assert_eq!(pg["default"], false);
    assert_eq!(pg["description"], "app");
    assert_eq!(pg["shadowed"], false);

    let kc = row("kc");
    assert_eq!(kc["dsn"], "keychain://kc");
    assert_eq!(kc["readonly"], true);

    let inspect = row("team/inspect");
    assert_eq!(inspect["dsn"], "keyvault://kv-team/dsn");
    assert_eq!(inspect["open_on_start"], true);
    assert_eq!(inspect["default"], true);
    assert_eq!(inspect["shadowed"], true);

    assert_eq!(row("query")["shadowed"], true);
    assert_eq!(row("source")["shadowed"], true);
    assert_eq!(row("here")["scope"], "project");

    json.stderr_has(
        "note: binsql query runs the command; open it with binsql -- query, binsql query or --conn query",
    )
    .stderr_has(
        "note: binsql inspect runs the command; open it with binsql -- inspect, binsql team/inspect or --conn team/inspect",
    )
    .stderr_has("note: binsql source alone still opens source");

    let table = fixture
        .binsql_with_project(&["source", "list"], &project)
        .succeeds()
        .stdout_has("name")
        .stdout_has("shadowed")
        .stdout_has("postgres://ann:");
    assert!(!table.stdout.contains("hunter2"), "{}", table.stdout);
    assert!(!table.stdout.contains("note:"), "{}", table.stdout);

    let quiet = fixture
        .binsql_with_project(&["source", "list", "-o", "none"], &project)
        .succeeds();
    assert!(quiet.stderr.is_empty(), "{}", quiet.stderr);
}

#[test]
fn source_show_finds_one_data_source_as_conn_does() {
    let fixture = Fixture::new("source-show");
    let project = write_sources(&fixture);

    let shown = fixture
        .binsql_with_project(&["source", "show", "inspect", "-o", "json"], &project)
        .succeeds()
        .stderr_has("note: binsql inspect runs the command");
    let parsed: serde_json::Value = serde_json::from_str(&shown.stdout).expect("valid json");
    assert_eq!(parsed["rows"].as_array().expect("rows").len(), 1);
    assert_eq!(parsed["rows"][0]["name"], "team/inspect");

    fixture
        .binsql_with_project(&["source", "show", "nope"], &project)
        .refused()
        .stderr_has("no saved data source named nope");
    fixture
        .binsql_with_project(&["source", "show"], &project)
        .refused();
    fixture
        .binsql_with_project(&["source", "list", "extra"], &project)
        .refused();
    fixture
        .binsql_with_project(&["source", "list", "--conn", "pg"], &project)
        .refused();
}

#[test]
fn source_default_prints_the_default_and_sets_it_in_its_own_file() {
    let fixture = Fixture::new("source-default");
    let project = write_sources(&fixture);
    let default_of = |run: Run| {
        let parsed: serde_json::Value = serde_json::from_str(&run.stdout).expect("valid json");
        assert_eq!(parsed["rows"].as_array().expect("rows").len(), 1);
        assert_eq!(parsed["rows"][0]["default"], true);
        parsed["rows"][0]["name"]
            .as_str()
            .expect("name")
            .to_string()
    };

    let shown = fixture
        .binsql_with_project(&["source", "default", "-o", "json"], &project)
        .succeeds();
    assert_eq!(default_of(shown), "team/inspect");

    let set = fixture
        .binsql_with_project(&["source", "default", "pg", "-o", "json"], &project)
        .succeeds();
    assert!(!set.stdout.contains("hunter2"), "{}", set.stdout);
    assert_eq!(default_of(set), "pg");
    let config = std::fs::read_to_string(fixture.config()).expect("read the config");
    assert!(config.contains(r#""default": "pg""#), "{config}");
    let shown = fixture
        .binsql_with_project(&["source", "default", "-o", "json"], &project)
        .succeeds();
    assert_eq!(default_of(shown), "pg");

    std::fs::write(
        &project,
        r#"{ "default": "here", "connections": { "here": { "driver": "sqlite", "dsn": "/tmp/h.db" } } }"#,
    )
    .expect("write the project config");
    fixture
        .binsql_with_project(&["source", "default", "kc"], &project)
        .failed()
        .stderr_has("--scope project");
    let config = std::fs::read_to_string(fixture.config()).expect("read the config");
    assert!(config.contains(r#""default": "pg""#), "{config}");
    fixture
        .binsql_with_project(&["source", "default", "kc", "--scope", "project"], &project)
        .succeeds();
    let written = std::fs::read_to_string(&project).expect("read the project config");
    assert!(written.contains(r#""default": "kc""#), "{written}");

    fixture
        .binsql_with_project(&["source", "default", "nope"], &project)
        .refused();
    fixture
        .binsql_with_project(&["source", "default", "pg", "kc"], &project)
        .refused();
    fixture
        .binsql_with_project(&["source", "default", "pg", "--scope", "other"], &project)
        .refused();
    fixture
        .binsql_with_project(&["source", "list", "--scope", "user"], &project)
        .refused();
}

#[test]
fn source_default_with_none_set_prints_nothing() {
    let fixture = Fixture::new("source-no-default");
    std::fs::write(
        fixture.config(),
        r#"{ "connections": { "pg": { "driver": "postgres", "dsn": "postgres://db/app" } } }"#,
    )
    .expect("write the config");
    for format in ["table", "json"] {
        let run = fixture
            .binsql(&["source", "default", "-o", format])
            .succeeds();
        assert_eq!(run.stdout, "", "{format}");
    }
}

/// A config of the sources `source test` is tried on, with `default` set.
fn write_testable(fixture: &Fixture, default: Option<&str>) {
    let database = fixture.database().display().to_string();
    let missing = fixture.directory.join("gone").join("app.db");
    let config = serde_json::json!({
        "default": default,
        "connections": {
            "good": { "driver": "sqlite", "dsn": database },
            "missing": { "driver": "sqlite", "dsn": missing.display().to_string() },
            "vault": { "driver": "mssql", "dsn": "https://kv-x.vault.azure.net/" },
            "pg": { "driver": "postgres", "dsn": "postgres://app:hunter2@127.0.0.1:1/app?sslmode=bogus" }
        }
    });
    std::fs::write(fixture.config(), config.to_string()).expect("write the config");
}

fn test_row(run: &Run) -> serde_json::Value {
    let parsed: serde_json::Value = serde_json::from_str(&run.stdout).expect("valid json");
    parsed["rows"][0].clone()
}

#[test]
fn source_test_says_whether_a_source_connects_and_where_it_stopped() {
    let fixture = Fixture::new("source-test");
    fixture.seed();
    write_testable(&fixture, None);

    let good = fixture
        .binsql(&["source", "test", "good", "-o", "json"])
        .succeeds();
    let row = test_row(&good);
    assert_eq!(row["name"], "good");
    assert_eq!(row["ok"], true);
    assert!(row["stage"].is_null() && row["error"].is_null(), "{row}");

    let missing = fixture
        .binsql(&["source", "test", "missing", "-o", "json"])
        .failed()
        .stderr_has("error:");
    let row = test_row(&missing);
    assert_eq!(row["ok"], false);
    assert_eq!(row["stage"], "connect");

    let vault = fixture
        .binsql(&["source", "test", "vault", "-o", "json"])
        .failed();
    assert_eq!(test_row(&vault)["stage"], "secret");

    let pg = fixture
        .binsql(&["source", "test", "pg", "-o", "json"])
        .failed();
    assert_eq!(test_row(&pg)["stage"], "connect");
    assert!(
        !pg.stdout.contains("hunter2") && !pg.stderr.contains("hunter2"),
        "{}{}",
        pg.stdout,
        pg.stderr
    );
}

#[test]
fn source_test_alone_tries_the_default() {
    let fixture = Fixture::new("source-test-default");
    fixture.seed();
    write_testable(&fixture, Some("good"));
    fixture
        .binsql(&["source", "test"])
        .succeeds()
        .stdout_has("good");

    write_testable(&fixture, None);
    fixture.binsql(&["source", "test"]).refused();
    fixture.binsql(&["source", "test", "good", "pg"]).refused();
    fixture.binsql(&["source", "list", "--fresh"]).refused();
    fixture.binsql(&["source", "clear-cache", "x"]).refused();
}

#[test]
fn source_clear_cache_removes_the_cached_secrets_and_their_key() {
    let fixture = Fixture::new("source-clear-cache");
    let cache = fixture.directory.join("secret-cache.json");
    let key = fixture.directory.join("cache.key");
    std::fs::write(&cache, "{}").expect("write the cache");
    std::fs::write(&key, "key").expect("write the key");

    let run = fixture.binsql(&["source", "clear-cache"]).succeeds();
    assert_eq!(run.stdout, "");
    assert!(!cache.exists() && !key.exists());
    fixture.binsql(&["source", "clear-cache"]).succeeds();
}

#[test]
fn a_file_and_stdin_are_both_read() {
    let fixture = Fixture::new("input");
    fixture.seed();

    let script: &Path = &fixture.directory.join("report.sql");
    std::fs::write(
        script,
        "-- yesterday\nSELECT name FROM artist ORDER BY name\n",
    )
    .expect("write the script");

    fixture
        .direct(&[
            "query",
            "--file",
            script.to_str().expect("utf-8 path"),
            "-o",
            "raw",
        ])
        .succeeds()
        .stdout_has("Autechre\nBoards of Canada\nPortishead\n");
}

/// The user and project files as they stand, to check a refusal wrote neither.
fn files(fixture: &Fixture, project: &str) -> (String, String) {
    (
        std::fs::read_to_string(fixture.config()).expect("read the config"),
        std::fs::read_to_string(project).expect("read the project config"),
    )
}

fn saved_row(run: Run) -> serde_json::Value {
    let parsed: serde_json::Value = serde_json::from_str(&run.stdout).expect("valid json");
    assert_eq!(parsed["rows"].as_array().expect("rows").len(), 1);
    parsed["rows"][0].clone()
}

#[test]
fn source_add_refuses_what_it_may_not_save_and_writes_nothing() {
    let fixture = Fixture::new("source-add-refused");
    let project = write_sources(&fixture);
    let before = files(&fixture, &project);
    let env = [("APP_DSN", "postgres://u:hunter2@h/db"), ("EMPTY", "")];
    let refused = |args: &[&str], message: &str| {
        let run = fixture
            .binsql_with_env(args, &project, &env)
            .refused()
            .stderr_has(message);
        assert!(!run.stderr.contains("hunter2"), "{}", run.stderr);
        assert_eq!(files(&fixture, &project), before, "{args:?}");
    };

    refused(
        &["source", "add", "new", "--dsn", "postgres://u:hunter2@h/db"],
        "--dsn-stdin or --dsn-env",
    );
    refused(
        &["source", "add", "new", "--dsn", "keychain://kc"],
        "a keychain:// reference names a secret binsql filed",
    );
    refused(
        &["source", "add", "pg", "--dsn", "./x.db"],
        "pg already exists; change it with binsql source edit pg",
    );
    for name in ["inspect", "exec"] {
        refused(
            &["source", "add", name, "--dsn", "./x.db"],
            &format!("{name} is a binsql command; save it inside a folder"),
        );
    }
    refused(
        &[
            "source",
            "add",
            "new",
            "--dsn-env",
            "APP_DSN",
            "--no-keychain",
            "--scope",
            "project",
        ],
        "a connection string is not written into .binsql.json",
    );
    refused(
        &["source", "add", "new", "--dsn-env", "UNSET_VAR"],
        "--dsn-env UNSET_VAR is not set",
    );
    refused(
        &["source", "add", "new", "--dsn-env", "EMPTY"],
        "--dsn-env EMPTY is not set",
    );
    refused(
        &["source", "add", "new"],
        "source add needs a connection string",
    );
    refused(
        &[
            "source",
            "add",
            "new",
            "--dsn",
            "./x.db",
            "--dsn-env",
            "APP_DSN",
        ],
        "give the connection string one way",
    );
    refused(
        &[
            "source",
            "add",
            "new",
            "--dsn-env",
            "APP_DSN",
            "--driver",
            "nope",
        ],
        "nope",
    );
    refused(
        &[
            "source",
            "add",
            "new",
            "--dsn",
            "keyvault://kv/s",
            "--readonly",
            "--no-readonly",
        ],
        "--readonly and --no-readonly cannot both be given",
    );
    refused(
        &[
            "source", "add", "new", "--dsn", "./x.db", "--rename", "other",
        ],
        "--rename applies only to source edit",
    );
    refused(
        &["source", "show", "pg", "--description", "x"],
        "--description applies only to source add and source edit",
    );
    refused(
        &["source", "list", "-d", "sqlite"],
        "--driver applies only to source add and source edit",
    );
    fixture
        .binsql_with_env(
            &["source", "add", "new", "--dsn-env", "NOT_A_DSN"],
            &project,
            &[("NOT_A_DSN", "not a dsn")],
        )
        .refused()
        .stderr_has("Could not tell the driver from that connection string — pick one");
    refused(
        &["source", "add", "a/b/c", "--dsn", "./x.db"],
        "'/' separates the folder from the name, so neither may contain it",
    );
    refused(
        &["source", "add", "a", "b", "--dsn", "./x.db"],
        "source add takes one name",
    );

    fixture
        .binsql(&[
            "source", "add", "new", "--dsn", "./x.db", "--scope", "project",
        ])
        .refused()
        .stderr_has("there is no .binsql.json here to save new in");
}

#[test]
fn source_add_saves_a_path_or_a_reference_as_written() {
    let fixture = Fixture::new("source-add");
    let project = write_sources(&fixture);

    let added = fixture
        .binsql_with_project(
            &[
                "source",
                "add",
                "team/source",
                "--dsn",
                "./x.db",
                "--readonly",
                "--description",
                "local copy",
                "-o",
                "json",
            ],
            &project,
        )
        .succeeds();
    let row = saved_row(added);
    assert_eq!(row["name"], "team/source");
    assert_eq!(row["driver"], "sqlite");
    assert_eq!(row["dsn"], "./x.db");
    assert_eq!(row["readonly"], true);
    assert_eq!(row["open_on_start"], false);
    assert_eq!(row["description"], "local copy");
    // A project file is in play, so a new source goes into it.
    assert_eq!(row["scope"], "project");
    let (_, written) = files(&fixture, &project);
    assert!(written.contains("./x.db"), "{written}");

    let added = fixture
        .binsql_with_project(
            &[
                "source",
                "add",
                "vault",
                "--dsn",
                "keyvault://kv/s",
                "-d",
                "mssql",
                "--scope",
                "project",
                "-o",
                "json",
            ],
            &project,
        )
        .succeeds();
    let row = saved_row(added);
    assert_eq!(row["dsn"], "keyvault://kv/s");
    assert_eq!(row["scope"], "project");
    let (_, written) = files(&fixture, &project);
    assert!(written.contains("keyvault://kv/s"), "{written}");
}

#[test]
fn source_add_with_no_keychain_keeps_the_string_in_the_user_file_and_masks_it() {
    let fixture = Fixture::new("source-add-literal");
    let project = write_sources(&fixture);
    let added = fixture
        .binsql_with_env(
            &[
                "source",
                "add",
                "app",
                "--dsn-env",
                "APP_DSN",
                "--no-keychain",
                "--scope",
                "user",
                "-o",
                "json",
            ],
            &project,
            &[("APP_DSN", "postgres://u:hunter2@h/db")],
        )
        .succeeds();
    assert!(!added.stdout.contains("hunter2"), "{}", added.stdout);
    let row = saved_row(added);
    assert_eq!(row["driver"], "postgres");
    assert_eq!(row["scope"], "user");
    let (config, _) = files(&fixture, &project);
    assert!(config.contains("postgres://u:hunter2@h/db"), "{config}");
}

#[test]
fn source_edit_changes_only_what_it_is_given() {
    let fixture = Fixture::new("source-edit");
    let project = write_sources(&fixture);

    let edited = fixture
        .binsql_with_project(
            &[
                "source",
                "edit",
                "team/inspect",
                "--description",
                "x",
                "-o",
                "json",
            ],
            &project,
        )
        .succeeds();
    let row = saved_row(edited);
    assert_eq!(row["name"], "team/inspect");
    assert_eq!(row["driver"], "mysql");
    assert_eq!(row["dsn"], "keyvault://kv-team/dsn");
    assert_eq!(row["readonly"], false);
    assert_eq!(row["open_on_start"], true);
    assert_eq!(row["scope"], "user");
    assert_eq!(row["description"], "x");

    let moved = fixture
        .binsql_with_project(
            &[
                "source",
                "edit",
                "query",
                "--scope",
                "project",
                "--no-readonly",
                "-o",
                "json",
            ],
            &project,
        )
        .succeeds();
    assert_eq!(saved_row(moved)["scope"], "project");
    let (config, written) = files(&fixture, &project);
    assert!(!config.contains("/tmp/q.db"), "{config}");
    assert!(written.contains("/tmp/q.db"), "{written}");

    let renamed = fixture
        .binsql_with_project(
            &[
                "source",
                "edit",
                "here",
                "--rename",
                "team/here",
                "-o",
                "json",
            ],
            &project,
        )
        .succeeds();
    let row = saved_row(renamed);
    assert_eq!(row["name"], "team/here");
    assert_eq!(row["dsn"], "/tmp/h.db");
    assert_eq!(row["scope"], "project");
    let (_, written) = files(&fixture, &project);
    let written: serde_json::Value = serde_json::from_str(&written).expect("valid json");
    assert!(written["connections"]["here"].is_null(), "{written}");
    assert_eq!(written["connections"]["team"]["here"]["dsn"], "/tmp/h.db");

    let before = files(&fixture, &project);
    fixture
        .binsql_with_project(&["source", "edit", "query", "--rename", "pg"], &project)
        .refused()
        .stderr_has("pg already exists");
    fixture
        .binsql_with_project(&["source", "edit", "nope", "--description", "x"], &project)
        .refused()
        .stderr_has("no saved data source named nope");
    fixture
        .binsql_with_project(
            &["source", "edit", "query", "--dsn", "keychain://kc"],
            &project,
        )
        .refused()
        .stderr_has("a keychain:// reference names a secret binsql filed");
    fixture
        .binsql_with_project(&["source", "edit", "pg", "--scope", "project"], &project)
        .refused()
        .stderr_has("a connection string is not written into .binsql.json");
    assert_eq!(files(&fixture, &project), before);
}

#[test]
fn source_remove_needs_force_and_a_saved_name() {
    let fixture = Fixture::new("source-remove-refused");
    let project = write_sources(&fixture);
    let before = files(&fixture, &project);
    let refused = |args: &[&str], message: &str| {
        fixture
            .binsql_with_project(args, &project)
            .refused()
            .stderr_has(message);
        assert_eq!(files(&fixture, &project), before, "{args:?}");
    };

    refused(&["source", "remove", "pg"], "add --force to do it");
    refused(
        &["source", "remove", "nothing", "--force"],
        "no saved data source named nothing",
    );
    refused(
        &["source", "remove", "--force"],
        "source remove takes one name",
    );
    refused(
        &["source", "list", "--force"],
        "--force applies only to source remove",
    );
}

#[test]
fn source_remove_takes_it_out_of_whichever_file_held_it() {
    let fixture = Fixture::new("source-remove");
    let project = write_sources(&fixture);

    let removed = fixture
        .binsql_with_project(
            &["source", "remove", "here", "--force", "-o", "json"],
            &project,
        )
        .succeeds();
    let row = saved_row(removed);
    assert_eq!(row["name"], "here");
    assert_eq!(row["scope"], "project");
    let (config, written) = files(&fixture, &project);
    assert!(!written.contains("/tmp/h.db"), "{written}");
    assert!(config.contains("ann:hunter2"), "{config}");

    let removed = fixture
        .binsql_with_project(
            &["source", "remove", "pg", "--force", "-o", "json"],
            &project,
        )
        .succeeds();
    let row = saved_row(removed);
    assert_eq!(row["scope"], "user");
    assert!(!row["dsn"].as_str().unwrap().contains("hunter2"), "{row}");
    let (config, _) = files(&fixture, &project);
    assert!(!config.contains("ann:hunter2"), "{config}");
    assert!(config.contains("/tmp/q.db"), "{config}");
}

/// The one error record a JSON failure is, checked to be the whole of stderr
/// and to leave stdout empty.
fn error_record(run: &Run) -> serde_json::Value {
    assert!(run.stdout.is_empty(), "stdout: {}", run.stdout);
    let lines: Vec<&str> = run.stderr.lines().collect();
    assert_eq!(lines.len(), 1, "one line on stderr:\n{}", run.stderr);
    let record: serde_json::Value = serde_json::from_str(lines[0]).expect("valid json");
    assert_eq!(record["type"], "error");
    assert_eq!(record["schema"], 1);
    assert_eq!(record["exit"], run.code);
    record
}

#[test]
fn error_format_json_makes_a_usage_error_one_record() {
    let fixture = Fixture::new("error-json-usage");
    let run = fixture
        .binsql(&["query", "SELECT 1", "--bogus", "--error-format", "json"])
        .refused();
    let record = error_record(&run);
    assert_eq!(record["category"], "usage");
    assert_eq!(record["phase"], "args");
    assert_eq!(record["message"], "unknown option --bogus");
}

#[test]
fn error_format_json_says_the_database_refused() {
    let fixture = Fixture::new("error-json-database");
    fixture.seed();
    let run = fixture
        .direct(&["query", "SELECT * FROM nowhere", "--error-format=json"])
        .failed();
    let record = error_record(&run);
    assert_eq!(record["category"], "database");
    assert_eq!(record["phase"], "execute");
    assert!(record["message"].as_str().unwrap().contains("nowhere"));
}

#[test]
fn error_format_json_keeps_the_sql_out_of_a_refusal() {
    let fixture = Fixture::new("error-json-refused");
    fixture.seed();
    let run = fixture
        .direct(&["query", "delete from artist", "--error-format", "json"])
        .refused();
    let record = error_record(&run);
    assert_eq!(record["category"], "refused");
    assert_eq!(record["phase"], "prepare");
    assert_eq!(record["statement"], 1);
    assert!(!run.stderr.contains("delete from artist"), "{}", run.stderr);
}

#[test]
fn error_format_json_counts_what_ran_before_a_failure_with_no_transaction() {
    let fixture = Fixture::new("error-json-no-tx");
    fixture.seed();
    let run = fixture
        .direct(&[
            "exec",
            "UPDATE artist SET founded = 1 WHERE id = 1; SELECT * FROM nowhere",
            "--no-tx",
            "--error-format",
            "json",
        ])
        .failed();
    let record = error_record(&run);
    assert_eq!(record["category"], "database");
    assert_eq!(record["statement"], 2);
    assert_eq!(record["completed"], 1);
}

#[test]
fn error_format_json_masks_the_password_in_a_connect_failure() {
    let fixture = Fixture::new("error-json-connect");
    let run = fixture
        .binsql(&[
            "query",
            "SELECT 1",
            "--dsn",
            "postgres://u:hunter2@127.0.0.1:1/x?sslmode=bogus",
            "--error-format",
            "json",
        ])
        .failed();
    let record = error_record(&run);
    assert_eq!(record["category"], "connect");
    assert_eq!(record["phase"], "connect");
    assert!(!run.stderr.contains("hunter2"), "{}", run.stderr);
}

#[test]
fn error_format_json_can_come_from_the_environment() {
    let fixture = Fixture::new("error-json-env");
    fixture.write_config();
    let run = fixture
        .binsql_with_env(
            &["query", "SELECT 1", "--conn", "nothing"],
            "",
            &[("BINSQL_ERROR_FORMAT", "json")],
        )
        .refused();
    let record = error_record(&run);
    assert_eq!(record["category"], "source");
    assert_eq!(record["phase"], "config");

    // The flag wins over the variable, and an empty variable is unset.
    for (flag, value) in [(&["--error-format", "text"][..], "json"), (&[][..], "")] {
        let mut args = vec!["query", "SELECT 1", "--conn", "nothing"];
        args.extend_from_slice(flag);
        let run = fixture
            .binsql_with_env(&args, "", &[("BINSQL_ERROR_FORMAT", value)])
            .refused();
        assert!(
            run.stderr
                .starts_with("error: no saved data source named nothing")
        );
    }
}

#[test]
fn a_bad_error_format_is_a_text_usage_error() {
    let fixture = Fixture::new("error-json-bad");
    let run = fixture
        .binsql(&["query", "SELECT 1", "--error-format", "yaml"])
        .refused();
    assert!(run.stdout.is_empty());
    assert!(
        run.stderr
            .starts_with("error: unknown error format yaml — one of: text, json\n"),
        "{}",
        run.stderr
    );
}

#[test]
fn text_errors_are_unchanged() {
    let fixture = Fixture::new("error-text");
    fixture.seed();
    let run = fixture.direct(&["query", "delete from artist"]).refused();
    assert_eq!(
        run.stderr,
        "error: refusing to run a write statement with `query`: use `binsql exec`, or --allow-write\n  \
         statement: delete from artist\n\nrun `binsql --help` for usage\n"
    );
}
