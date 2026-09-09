package drivers

import (
	"os"
	"path/filepath"
	"testing"
)

func TestParse(t *testing.T) {
	tests := map[string]Driver{
		"sqlite":     DriverSqlite,
		"sqlite3":    DriverSqlite,
		"postgres":   DriverPostgres,
		"postgresql": DriverPostgres,
		"pg":         DriverPostgres,
		"mssql":      DriverMssql,
		"sqlserver":  DriverMssql,
		"mysql":      DriverMysql,
		"mariadb":    DriverMysql,
		"  MySQL  ":  DriverMysql,
	}
	for in, want := range tests {
		got, err := Parse(in)
		if err != nil || got != want {
			t.Errorf("Parse(%q) = (%q, %v), want %q", in, got, err, want)
		}
	}

	if _, err := Parse("oracle"); err == nil {
		t.Error("expected an error for an unsupported driver")
	}
}

func TestInfer(t *testing.T) {
	tests := []struct {
		dsn  string
		want Driver
	}{
		{"postgres://user:pass@host:5432/db", DriverPostgres},
		{"postgresql://host/db", DriverPostgres},
		{"host=localhost dbname=app sslmode=disable", DriverPostgres},
		{"sqlserver://user:pass@host:1433?database=db", DriverMssql},
		{"server=host;database=db;encrypt=true", DriverMssql},
		{"server=x;database=y;fedauth=ActiveDirectoryAzCli", DriverMssql},
		{"user:pass@tcp(localhost:3306)/db?parseTime=true", DriverMysql},
		{"user@tcp(host)/db", DriverMysql},
		{"mysql://user:pass@host/db", DriverMysql},
		{"./app.db", DriverSqlite},
		{"/var/data/cms.sqlite", DriverSqlite},
		{"data.sqlite3", DriverSqlite},
		{"file:memory.db", DriverSqlite},
		{"", ""},
		{"something-ambiguous", ""},
	}

	for _, tt := range tests {
		t.Run(tt.dsn, func(t *testing.T) {
			if got := Infer(tt.dsn); got != tt.want {
				t.Errorf("Infer(%q) = %q, want %q", tt.dsn, got, tt.want)
			}
		})
	}
}

// An extension-less path is only sqlite if it actually exists on disk.
func TestInferExistingFile(t *testing.T) {
	path := filepath.Join(t.TempDir(), "database")
	if got := Infer(path); got != "" {
		t.Errorf("Infer(%q) = %q before the file exists, want empty", path, got)
	}

	if err := writeEmpty(path); err != nil {
		t.Fatal(err)
	}
	if got := Infer(path); got != DriverSqlite {
		t.Errorf("Infer(%q) = %q once the file exists, want sqlite", path, got)
	}
}

func TestIsName(t *testing.T) {
	if !IsName("postgres") {
		t.Error("postgres should be recognised as a driver name")
	}
	// Command names must not be mistaken for drivers, or the legacy
	// invocation check in main would misroute them.
	for _, cmd := range []string{"query", "exec", "tables", "schema", "conn", "help"} {
		if IsName(cmd) {
			t.Errorf("%q must not be treated as a driver name", cmd)
		}
	}
}

func writeEmpty(path string) error {
	return os.WriteFile(path, nil, 0o600)
}
