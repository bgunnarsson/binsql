// Package config stores named connection profiles so the CLI can be driven
// with `--conn prod` instead of a full DSN on every invocation.
package config

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// Connection is one saved profile.
type Connection struct {
	Driver      string `json:"driver"`
	DSN         string `json:"dsn"`
	Description string `json:"description,omitempty"`
	// ReadOnly blocks every mutating statement on this connection, which is
	// how a production database should be registered.
	ReadOnly bool `json:"readonly,omitempty"`
}

// Config is the on-disk file.
type Config struct {
	Default     string                `json:"default,omitempty"`
	Connections map[string]Connection `json:"connections"`
}

// Path returns the config file location, honouring BINSQL_CONFIG and
// XDG_CONFIG_HOME.
func Path() string {
	if p := os.Getenv("BINSQL_CONFIG"); p != "" {
		return p
	}
	if dir := os.Getenv("XDG_CONFIG_HOME"); dir != "" {
		return filepath.Join(dir, "binsql", "connections.json")
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return "binsql-connections.json"
	}
	return filepath.Join(home, ".config", "binsql", "connections.json")
}

// Dir is the directory holding the config and the secret cache.
func Dir() string { return filepath.Dir(Path()) }

// Load reads the config, returning an empty one if the file does not exist.
func Load() (*Config, error) {
	cfg := &Config{Connections: map[string]Connection{}}

	data, err := os.ReadFile(Path())
	if err != nil {
		if os.IsNotExist(err) {
			return cfg, nil
		}
		return nil, err
	}
	if len(strings.TrimSpace(string(data))) == 0 {
		return cfg, nil
	}
	if err := json.Unmarshal(data, cfg); err != nil {
		return nil, fmt.Errorf("parsing %s: %w", Path(), err)
	}
	if cfg.Connections == nil {
		cfg.Connections = map[string]Connection{}
	}
	return cfg, nil
}

// Save writes the config with owner-only permissions, since it holds secrets.
func (c *Config) Save() error {
	path := Path()
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}

	data, err := json.MarshalIndent(c, "", "  ")
	if err != nil {
		return err
	}
	data = append(data, '\n')

	// Write to a temp file first so a failure cannot truncate the original.
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, data, 0o600); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}

// Get looks up a connection by name.
func (c *Config) Get(name string) (Connection, bool) {
	conn, ok := c.Connections[name]
	return conn, ok
}

// Names returns every profile name, sorted.
func (c *Config) Names() []string {
	out := make([]string, 0, len(c.Connections))
	for name := range c.Connections {
		out = append(out, name)
	}
	sort.Strings(out)
	return out
}

// Set adds or replaces a profile.
func (c *Config) Set(name string, conn Connection) {
	if c.Connections == nil {
		c.Connections = map[string]Connection{}
	}
	c.Connections[name] = conn
}

// Remove deletes a profile, clearing the default if it pointed there.
func (c *Config) Remove(name string) bool {
	if _, ok := c.Connections[name]; !ok {
		return false
	}
	delete(c.Connections, name)
	if c.Default == name {
		c.Default = ""
	}
	return true
}

var passwordPattern = regexp.MustCompile(`(?i)((?:password|pwd)\s*=)[^;\s]*`)

// MaskDSN hides credentials so a DSN can be printed or logged safely.
func MaskDSN(dsn string) string {
	masked := passwordPattern.ReplaceAllString(dsn, "${1}****")

	// URL form: scheme://user:password@host
	if i := strings.Index(masked, "://"); i >= 0 {
		rest := masked[i+3:]
		if at := strings.LastIndex(rest, "@"); at > 0 {
			creds := rest[:at]
			if colon := strings.Index(creds, ":"); colon >= 0 {
				masked = masked[:i+3] + creds[:colon] + ":****" + rest[at:]
			}
		}
		return masked
	}

	// MySQL form: user:password@tcp(...)
	if at := strings.LastIndex(masked, "@"); at > 0 {
		creds := masked[:at]
		if colon := strings.Index(creds, ":"); colon >= 0 && !strings.Contains(creds, " ") {
			masked = creds[:colon] + ":****" + masked[at:]
		}
	}
	return masked
}
