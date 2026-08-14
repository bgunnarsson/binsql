package config

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestMaskDSN(t *testing.T) {
	tests := []struct {
		name string
		dsn  string
		want string
	}{
		{
			name: "postgres url",
			dsn:  "postgres://admin:hunter2@db.example.com:5432/app",
			want: "postgres://admin:****@db.example.com:5432/app",
		},
		{
			name: "mysql dsn",
			dsn:  "user:s3cret@tcp(localhost:3306)/shop",
			want: "user:****@tcp(localhost:3306)/shop",
		},
		{
			name: "sqlserver keyword form",
			dsn:  "server=host;database=db;password=letmein;encrypt=true",
			want: "server=host;database=db;password=****;encrypt=true",
		},
		{
			name: "sqlserver pwd alias",
			dsn:  "server=host;pwd=letmein",
			want: "server=host;pwd=****",
		},
		{
			name: "sqlite path is untouched",
			dsn:  "./app.db",
			want: "./app.db",
		},
		{
			name: "url without a password",
			dsn:  "postgres://admin@db.example.com/app",
			want: "postgres://admin@db.example.com/app",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := MaskDSN(tt.dsn); got != tt.want {
				t.Errorf("MaskDSN(%q) = %q, want %q", tt.dsn, got, tt.want)
			}
		})
	}
}

func TestMaskDSNHidesEverySecret(t *testing.T) {
	secrets := []string{"hunter2", "s3cret", "letmein"}
	dsns := []string{
		"postgres://admin:hunter2@host/db",
		"user:s3cret@tcp(host:3306)/db",
		"server=host;password=letmein",
	}
	for _, dsn := range dsns {
		masked := MaskDSN(dsn)
		for _, s := range secrets {
			if strings.Contains(masked, s) {
				t.Errorf("MaskDSN(%q) leaked %q: %s", dsn, s, masked)
			}
		}
	}
}

func TestSaveAndLoadRoundTrip(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "connections.json")
	t.Setenv("BINSQL_CONFIG", path)

	cfg, err := Load()
	if err != nil {
		t.Fatal(err)
	}
	if len(cfg.Connections) != 0 {
		t.Errorf("a missing file should load as empty, got %d entries", len(cfg.Connections))
	}

	cfg.Set("local", Connection{Driver: "sqlite", DSN: "./app.db"})
	cfg.Set("prod", Connection{Driver: "postgres", DSN: "postgres://h/db", ReadOnly: true})
	cfg.Default = "local"
	if err := cfg.Save(); err != nil {
		t.Fatal(err)
	}

	// The file holds credentials, so it must not be group- or world-readable.
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if perm := info.Mode().Perm(); perm != 0o600 {
		t.Errorf("config permissions = %o, want 600", perm)
	}

	reloaded, err := Load()
	if err != nil {
		t.Fatal(err)
	}
	if reloaded.Default != "local" {
		t.Errorf("default = %q", reloaded.Default)
	}
	prod, ok := reloaded.Get("prod")
	if !ok || !prod.ReadOnly || prod.Driver != "postgres" {
		t.Errorf("prod round-trip failed: %+v", prod)
	}
	if got := reloaded.Names(); len(got) != 2 || got[0] != "local" || got[1] != "prod" {
		t.Errorf("Names() = %v, want sorted [local prod]", got)
	}
}

func TestRemoveClearsDefault(t *testing.T) {
	t.Setenv("BINSQL_CONFIG", filepath.Join(t.TempDir(), "connections.json"))

	cfg := &Config{Connections: map[string]Connection{}}
	cfg.Set("only", Connection{Driver: "sqlite", DSN: "./a.db"})
	cfg.Default = "only"

	if !cfg.Remove("only") {
		t.Fatal("Remove() reported no such connection")
	}
	if cfg.Default != "" {
		t.Errorf("default should be cleared, got %q", cfg.Default)
	}
	if cfg.Remove("only") {
		t.Error("removing twice should report false")
	}
}
