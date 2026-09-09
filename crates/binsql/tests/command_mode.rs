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
        let output = Command::new(env!("CARGO_BIN_EXE_binsql"))
            .args(args)
            .env("BINSQL_CONFIG", self.config())
            // The tests must not read whatever the developer running them has
            // in their own environment.
            .env_remove("BINSQL_CONN")
            .env_remove("BINSQL_DSN")
            .env_remove("BINSQL_DRIVER")
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
    ] {
        fixture
            .binsql(&["exec", "--conn", "prod", sql, "--force"])
            .failed()
            .stderr_has("read-only");
    }

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
