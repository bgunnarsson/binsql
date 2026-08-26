package cli

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// run invokes a command exactly as the binary would, capturing both streams.
func run(t *testing.T, args ...string) (stdout, stderr string, code int) {
	t.Helper()

	outFile, err := os.CreateTemp(t.TempDir(), "stdout")
	if err != nil {
		t.Fatal(err)
	}
	errFile, err := os.CreateTemp(t.TempDir(), "stderr")
	if err != nil {
		t.Fatal(err)
	}

	origOut, origErr := os.Stdout, os.Stderr
	os.Stdout, os.Stderr = outFile, errFile
	code = Main(context.Background(), args)
	os.Stdout, os.Stderr = origOut, origErr

	_ = outFile.Close()
	_ = errFile.Close()

	o, _ := os.ReadFile(outFile.Name())
	e, _ := os.ReadFile(errFile.Name())
	return string(o), string(e), code
}

// newDB creates a throwaway sqlite database and points the environment at it.
func newDB(t *testing.T) string {
	t.Helper()

	dir := t.TempDir()
	dsn := filepath.Join(dir, "test.db")

	t.Setenv("BINSQL_CONFIG", filepath.Join(dir, "connections.json"))
	t.Setenv("BINSQL_DSN", dsn)
	t.Setenv("BINSQL_CONN", "")
	t.Setenv("BINSQL_FORMAT", "")
	t.Setenv("BINSQL_READONLY", "")

	_, stderr, code := run(t, "exec", `
		create table users (id integer primary key, email text not null unique, active integer default 1);
		insert into users (id, email) values (1, 'a@example.com'), (2, 'b@example.com'), (3, 'c@example.com');
	`)
	if code != 0 {
		t.Fatalf("seeding failed (%d): %s", code, stderr)
	}
	return dsn
}

func TestQueryJSON(t *testing.T) {
	newDB(t)

	stdout, stderr, code := run(t, "query", "select id, email from users order by id", "-o", "json")
	if code != 0 {
		t.Fatalf("exit %d: %s", code, stderr)
	}

	var out struct {
		Rows  []map[string]any `json:"rows"`
		Count int              `json:"row_count"`
	}
	if err := json.Unmarshal([]byte(stdout), &out); err != nil {
		t.Fatalf("not JSON: %v\n%s", err, stdout)
	}
	if out.Count != 3 {
		t.Errorf("row_count = %d, want 3", out.Count)
	}
	if out.Rows[0]["email"] != "a@example.com" {
		t.Errorf("unexpected first row: %#v", out.Rows[0])
	}
}

// Flags after the positional SQL must still be honoured.
func TestFlagsAfterPositionalArgument(t *testing.T) {
	newDB(t)

	stdout, stderr, code := run(t, "query", "select id from users where id = ?", "--arg", "int:2", "-o", "raw")
	if code != 0 {
		t.Fatalf("exit %d: %s", code, stderr)
	}
	if strings.TrimSpace(stdout) != "2" {
		t.Errorf("stdout = %q, want \"2\"", stdout)
	}
}

func TestQueryRefusesWrites(t *testing.T) {
	newDB(t)

	_, stderr, code := run(t, "query", "delete from users where id = 1")
	if code == 0 {
		t.Fatal("expected a non-zero exit")
	}
	if !strings.Contains(stderr, "binsql exec") {
		t.Errorf("error should point at exec: %s", stderr)
	}

	// The row must still be there.
	stdout, _, _ := run(t, "count", "users", "-o", "raw")
	if strings.TrimSpace(stdout) != "3" {
		t.Errorf("rows were deleted: count = %q", stdout)
	}
}

func TestQueryAllowWriteOverride(t *testing.T) {
	newDB(t)

	_, stderr, code := run(t, "query", "delete from users where id = 1", "--allow-write")
	if code != 0 {
		t.Fatalf("exit %d: %s", code, stderr)
	}
	stdout, _, _ := run(t, "count", "users", "-o", "raw")
	if strings.TrimSpace(stdout) != "2" {
		t.Errorf("count = %q, want 2", stdout)
	}
}

func TestExecGuardsUnqualifiedDelete(t *testing.T) {
	newDB(t)

	_, stderr, code := run(t, "exec", "delete from users")
	if code == 0 {
		t.Fatal("expected a non-zero exit")
	}
	if !strings.Contains(stderr, "--force") {
		t.Errorf("error should mention --force: %s", stderr)
	}

	stdout, _, _ := run(t, "count", "users", "-o", "raw")
	if strings.TrimSpace(stdout) != "3" {
		t.Errorf("rows were deleted despite the guard: %q", stdout)
	}
}

func TestExecForceAllowsUnqualifiedDelete(t *testing.T) {
	newDB(t)

	if _, stderr, code := run(t, "exec", "delete from users", "--force"); code != 0 {
		t.Fatalf("exit %d: %s", code, stderr)
	}
	stdout, _, _ := run(t, "count", "users", "-o", "raw")
	if strings.TrimSpace(stdout) != "0" {
		t.Errorf("count = %q, want 0", stdout)
	}
}

func TestExecDryRunRollsBack(t *testing.T) {
	newDB(t)

	_, stderr, code := run(t, "exec", "update users set active = 0 where id = 1", "--dry-run")
	if code != 0 {
		t.Fatalf("exit %d: %s", code, stderr)
	}

	stdout, _, _ := run(t, "query", "select active from users where id = 1", "-o", "raw")
	if strings.TrimSpace(stdout) != "1" {
		t.Errorf("dry run was committed: active = %q", stdout)
	}
}

// A batch must be atomic: a failure part-way through undoes the earlier work.
func TestExecBatchRollsBackOnError(t *testing.T) {
	newDB(t)

	_, _, code := run(t, "exec",
		"insert into users (id, email) values (4, 'd@example.com'); insert into users (id, email) values (5, 'a@example.com');")
	if code == 0 {
		t.Fatal("expected the duplicate email to fail the batch")
	}

	stdout, _, _ := run(t, "count", "users", "-o", "raw")
	if strings.TrimSpace(stdout) != "3" {
		t.Errorf("partial write survived: count = %q, want 3", stdout)
	}
}

func TestReadOnlyConnectionRefusesWrites(t *testing.T) {
	dsn := newDB(t)
	t.Setenv("BINSQL_DSN", "")

	if _, stderr, code := run(t, "conn", "add", "ro", "--dsn", dsn, "--readonly"); code != 0 {
		t.Fatalf("conn add failed (%d): %s", code, stderr)
	}

	_, stderr, code := run(t, "exec", "--conn", "ro", "update users set active = 0 where id = 1")
	if code == 0 {
		t.Fatal("expected the read-only guard to reject the write")
	}
	if !strings.Contains(stderr, "read-only") {
		t.Errorf("unexpected error: %s", stderr)
	}

	// Reads still work on the same profile.
	stdout, stderr, code := run(t, "count", "users", "--conn", "ro", "-o", "raw")
	if code != 0 {
		t.Fatalf("read on a read-only connection failed (%d): %s", code, stderr)
	}
	if strings.TrimSpace(stdout) != "3" {
		t.Errorf("count = %q", stdout)
	}
}

func TestSchemaJSON(t *testing.T) {
	newDB(t)

	stdout, stderr, code := run(t, "schema", "-o", "json")
	if code != 0 {
		t.Fatalf("exit %d: %s", code, stderr)
	}

	var out struct {
		Tables []struct {
			Table   string `json:"table"`
			Columns []struct {
				Name       string `json:"name"`
				Type       string `json:"type"`
				PrimaryKey bool   `json:"primary_key"`
			} `json:"columns"`
		} `json:"tables"`
	}
	if err := json.Unmarshal([]byte(stdout), &out); err != nil {
		t.Fatalf("not JSON: %v\n%s", err, stdout)
	}
	if len(out.Tables) != 1 || out.Tables[0].Table != "users" {
		t.Fatalf("unexpected tables: %+v", out.Tables)
	}
	cols := out.Tables[0].Columns
	if len(cols) != 3 || cols[0].Name != "id" || !cols[0].PrimaryKey {
		t.Errorf("unexpected columns: %+v", cols)
	}
}

func TestPlaceholderCountMismatch(t *testing.T) {
	newDB(t)

	_, stderr, code := run(t, "query", "select * from users where id = ? and email = ?", "--arg", "int:1")
	if code != exitUsage {
		t.Fatalf("exit = %d, want %d", code, exitUsage)
	}
	if !strings.Contains(stderr, "placeholder") {
		t.Errorf("unexpected error: %s", stderr)
	}
}

func TestUnknownConnectionIsAUsageError(t *testing.T) {
	newDB(t)
	t.Setenv("BINSQL_DSN", "")

	_, stderr, code := run(t, "tables", "--conn", "nope")
	if code != exitUsage {
		t.Fatalf("exit = %d, want %d", code, exitUsage)
	}
	if !strings.Contains(stderr, "nope") {
		t.Errorf("unexpected error: %s", stderr)
	}
}

func TestErrorsAreJSONWhenFormatIsJSON(t *testing.T) {
	newDB(t)

	_, stderr, code := run(t, "query", "select * from missing_table", "-o", "json")
	if code == 0 {
		t.Fatal("expected a non-zero exit")
	}

	var out struct {
		Error string `json:"error"`
	}
	if err := json.Unmarshal([]byte(stderr), &out); err != nil {
		t.Fatalf("stderr is not JSON: %v\n%s", err, stderr)
	}
	if out.Error == "" {
		t.Error("expected a populated error field")
	}
}

// --- Azure Key Vault references ------------------------------------------

// --no-verify skips the vault round-trip, which means the driver can no
// longer be discovered and must be supplied.
func TestKeyVaultProfileNeedsDriverWhenUnverified(t *testing.T) {
	newDB(t)
	t.Setenv("BINSQL_DSN", "")

	_, stderr, code := run(t, "conn", "add", "kv", "--dsn", "keyvault://my-vault/conn", "--no-verify")
	if code != exitUsage {
		t.Fatalf("exit = %d, want %d (%s)", code, exitUsage, stderr)
	}
	if !strings.Contains(stderr, "--driver") {
		t.Errorf("error should ask for --driver: %s", stderr)
	}
}

// Pasting the vault URL without a secret name must be caught, and the error
// must name the real problem rather than blaming driver inference.
func TestVaultUrlWithoutSecretIsRejected(t *testing.T) {
	newDB(t)
	t.Setenv("BINSQL_DSN", "")

	_, stderr, code := run(t, "conn", "add", "prod",
		"--dsn", "https://kv-example.vault.azure.net", "--readonly")
	if code != exitUsage {
		t.Fatalf("exit = %d, want %d (%s)", code, exitUsage, stderr)
	}
	if !strings.Contains(stderr, "not a secret inside it") {
		t.Errorf("error should explain the real problem: %s", stderr)
	}
	if strings.Contains(stderr, "cannot infer the driver") {
		t.Errorf("error should not blame driver inference: %s", stderr)
	}
	if !strings.Contains(stderr, "az keyvault secret list --vault-name kv-example") {
		t.Errorf("error should show how to list the secrets: %s", stderr)
	}
}

// Passing --driver skips inference, so the guard has to run independently of
// it — otherwise a vault URL gets stored as if it were a connection string.
func TestVaultUrlRejectedEvenWithExplicitDriver(t *testing.T) {
	newDB(t)
	t.Setenv("BINSQL_DSN", "")

	_, stderr, code := run(t, "conn", "add", "prod",
		"--dsn", "https://kv-example.vault.azure.net/ConnectionStrings--Db",
		"--driver", "mssql", "--readonly")
	if code != exitUsage {
		t.Fatalf("exit = %d, want %d — a vault URL was accepted as a DSN (%s)", code, exitUsage, stderr)
	}
	if !strings.Contains(stderr, "not a secret inside it") {
		t.Errorf("unexpected error: %s", stderr)
	}

	// And nothing should have been written.
	if data, err := os.ReadFile(os.Getenv("BINSQL_CONFIG")); err == nil {
		if strings.Contains(string(data), "kv-example") {
			t.Errorf("a bad profile was saved: %s", data)
		}
	}
}

func TestKeyVaultProfileStoresNoSecret(t *testing.T) {
	newDB(t)
	t.Setenv("BINSQL_DSN", "")

	_, stderr, code := run(t, "conn", "add", "kv",
		"--dsn", "keyvault://my-vault/sql-conn", "--driver", "postgres", "--no-verify", "--readonly")
	if code != 0 {
		t.Fatalf("conn add failed (%d): %s", code, stderr)
	}

	// The config must hold the reference, never a credential.
	data, err := os.ReadFile(os.Getenv("BINSQL_CONFIG"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(data), "keyvault://my-vault/sql-conn") {
		t.Errorf("reference not stored: %s", data)
	}

	stdout, _, code := run(t, "conn", "list")
	if code != 0 {
		t.Fatal("conn list failed")
	}
	if !strings.Contains(stdout, "keyvault://my-vault/sql-conn") {
		t.Errorf("conn list should show the reference: %s", stdout)
	}
}

// The read-only guard must reject a write on a vault-backed profile without
// ever contacting Azure — the driver recorded at `conn add` time is what
// makes that possible.
func TestKeyVaultReadOnlyGuardRunsBeforeResolving(t *testing.T) {
	newDB(t)
	t.Setenv("BINSQL_DSN", "")
	t.Setenv("BINSQL_SECRET_TTL", "0")

	if _, stderr, code := run(t, "conn", "add", "kv",
		"--dsn", "keyvault://my-vault/sql-conn", "--driver", "postgres", "--no-verify", "--readonly"); code != 0 {
		t.Fatalf("conn add failed (%d): %s", code, stderr)
	}

	done := make(chan struct{})
	var stderr string
	var code int
	go func() {
		_, stderr, code = run(t, "exec", "--conn", "kv", "update t set a = 1 where id = 2")
		close(done)
	}()

	select {
	case <-done:
	case <-time.After(10 * time.Second):
		t.Fatal("the guard should have rejected the write without any network call")
	}

	if code == 0 {
		t.Fatal("expected the read-only guard to reject the write")
	}
	if !strings.Contains(stderr, "read-only") {
		t.Errorf("unexpected error: %s", stderr)
	}
}

func TestConnCacheReportsAndClears(t *testing.T) {
	newDB(t)

	_, stderr, code := run(t, "conn", "cache")
	if code != 0 {
		t.Fatalf("exit %d: %s", code, stderr)
	}
	if !strings.Contains(stderr, "cached secrets:") {
		t.Errorf("unexpected output: %s", stderr)
	}

	if _, stderr, code := run(t, "conn", "cache", "--clear"); code != 0 {
		t.Fatalf("clear failed (%d): %s", code, stderr)
	}
}

func TestIsCommandDoesNotClaimDriverNames(t *testing.T) {
	for _, name := range []string{"sqlite", "postgres", "mssql", "mysql"} {
		if IsCommand(name) {
			t.Errorf("%q must not be treated as a command, or the legacy form breaks", name)
		}
	}
	for _, name := range []string{"query", "exec", "tables", "help"} {
		if !IsCommand(name) {
			t.Errorf("%q should be a command", name)
		}
	}
}
