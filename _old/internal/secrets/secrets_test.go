package secrets

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestIsRef(t *testing.T) {
	refs := []string{
		"keyvault://my-vault/secret",
		"KEYVAULT://my-vault/secret",
		"azkv://my-vault/secret",
		"https://my-vault.vault.azure.net/secrets/conn",
	}
	for _, r := range refs {
		if !IsRef(r) {
			t.Errorf("IsRef(%q) = false, want true", r)
		}
	}

	// Real connection strings must never be mistaken for references.
	literals := []string{
		"./app.db",
		"postgres://user:pass@host:5432/db",
		"sqlserver://user:pass@host?database=db",
		"user:pass@tcp(localhost:3306)/db",
		"server=host;database=db;fedauth=ActiveDirectoryAzCli",
		"",
	}
	for _, l := range literals {
		if IsRef(l) {
			t.Errorf("IsRef(%q) = true, want false", l)
		}
	}
}

func TestParse(t *testing.T) {
	tests := []struct {
		name string
		in   string
		want Ref
	}{
		{
			name: "short form",
			in:   "keyvault://my-vault/sql-conn",
			want: Ref{VaultHost: "my-vault.vault.azure.net", Name: "sql-conn"},
		},
		{
			name: "short form with version",
			in:   "keyvault://my-vault/sql-conn/abc123",
			want: Ref{VaultHost: "my-vault.vault.azure.net", Name: "sql-conn", Version: "abc123"},
		},
		{
			name: "explicit host",
			in:   "keyvault://my-vault.vault.usgovcloudapi.net/sql-conn",
			want: Ref{VaultHost: "my-vault.vault.usgovcloudapi.net", Name: "sql-conn"},
		},
		{
			name: "azkv alias",
			in:   "azkv://my-vault/sql-conn",
			want: Ref{VaultHost: "my-vault.vault.azure.net", Name: "sql-conn"},
		},
		{
			name: "portal secret identifier",
			in:   "https://my-vault.vault.azure.net/secrets/sql-conn",
			want: Ref{VaultHost: "my-vault.vault.azure.net", Name: "sql-conn"},
		},
		{
			name: "portal identifier with version",
			in:   "https://my-vault.vault.azure.net/secrets/sql-conn/9f8e7d",
			want: Ref{VaultHost: "my-vault.vault.azure.net", Name: "sql-conn", Version: "9f8e7d"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := Parse(tt.in, "")
			if err != nil {
				t.Fatalf("Parse(%q) error: %v", tt.in, err)
			}
			if got != tt.want {
				t.Errorf("Parse(%q) = %+v, want %+v", tt.in, got, tt.want)
			}
		})
	}
}

func TestParseCustomSuffix(t *testing.T) {
	got, err := Parse("keyvault://my-vault/secret", "vault.usgovcloudapi.net")
	if err != nil {
		t.Fatal(err)
	}
	if got.VaultHost != "my-vault.vault.usgovcloudapi.net" {
		t.Errorf("VaultHost = %q", got.VaultHost)
	}
}

func TestParseRejectsMalformed(t *testing.T) {
	bad := []string{
		"keyvault://my-vault",
		"keyvault://my-vault/a/b/c/d",
		"keyvault:///secret",
		"https://my-vault.vault.azure.net/keys/mykey",
		"postgres://host/db",
	}
	for _, in := range bad {
		if _, err := Parse(in, ""); err == nil {
			t.Errorf("Parse(%q) should have failed", in)
		}
	}
}

func TestRefStringIsRoundTrippable(t *testing.T) {
	in := "keyvault://my-vault.vault.azure.net/sql-conn/v1"
	ref, err := Parse(in, "")
	if err != nil {
		t.Fatal(err)
	}
	again, err := Parse(ref.String(), "")
	if err != nil {
		t.Fatal(err)
	}
	if again != ref {
		t.Errorf("round trip changed the reference: %+v vs %+v", again, ref)
	}
}

func TestVaultOnly(t *testing.T) {
	// Naming a vault but no secret — the mistake worth catching.
	incomplete := map[string]string{
		"https://kv-eimskip-prd.vault.azure.net":      "kv-eimskip-prd.vault.azure.net",
		"https://kv-eimskip-prd.vault.azure.net/":     "kv-eimskip-prd.vault.azure.net",
		"https://my-vault.vault.usgovcloudapi.net":    "my-vault.vault.usgovcloudapi.net",
		"https://my-vault.vault.azure.net/secrets":    "my-vault.vault.azure.net",
		"keyvault://my-vault":                         "my-vault.vault.azure.net",
		"keyvault://my-vault.vault.usgovcloudapi.net": "my-vault.vault.usgovcloudapi.net",
	}
	for in, wantHost := range incomplete {
		host, ok := VaultOnly(in)
		if !ok || host != wantHost {
			t.Errorf("VaultOnly(%q) = (%q, %v), want (%q, true)", in, host, ok, wantHost)
		}
	}

	// Complete references and ordinary DSNs must not be flagged.
	complete := []string{
		"https://my-vault.vault.azure.net/secrets/conn",
		"https://my-vault.vault.azure.net/secrets/conn/v1",
		"keyvault://my-vault/conn",
		"postgres://user:pw@host/db",
		"./app.db",
		"https://example.com",
		"",
	}
	for _, in := range complete {
		if host, ok := VaultOnly(in); ok {
			t.Errorf("VaultOnly(%q) = (%q, true), want false", in, host)
		}
	}
}

func TestVaultName(t *testing.T) {
	tests := map[string]string{
		"kv-eimskip-prd.vault.azure.net":   "kv-eimskip-prd",
		"my-vault.vault.usgovcloudapi.net": "my-vault",
		"my-vault":                         "my-vault",
	}
	for in, want := range tests {
		if got := VaultName(in); got != want {
			t.Errorf("VaultName(%q) = %q, want %q", in, got, want)
		}
	}
}

// --- cache ---------------------------------------------------------------

func newCache(t *testing.T, ttl time.Duration) *Cache {
	t.Helper()
	return &Cache{Dir: t.TempDir(), TTL: ttl}
}

func TestCacheRoundTrip(t *testing.T) {
	c := newCache(t, time.Minute)
	ref := Ref{VaultHost: "v.vault.azure.net", Name: "conn"}

	if _, ok := c.Get(ref); ok {
		t.Fatal("empty cache should miss")
	}
	if err := c.Put(ref, "postgres://user:pw@host/db"); err != nil {
		t.Fatal(err)
	}

	got, ok := c.Get(ref)
	if !ok || got != "postgres://user:pw@host/db" {
		t.Errorf("Get() = (%q, %v)", got, ok)
	}
}

// The whole point of encrypting is that the secret is not sitting in the file
// in the clear.
func TestCacheFileDoesNotContainPlaintext(t *testing.T) {
	c := newCache(t, time.Minute)
	ref := Ref{VaultHost: "v.vault.azure.net", Name: "conn"}
	const secret = "postgres://user:sup3rs3cret@host/db"

	if err := c.Put(ref, secret); err != nil {
		t.Fatal(err)
	}

	data, err := os.ReadFile(filepath.Join(c.Dir, "secret-cache.json"))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(data), "sup3rs3cret") {
		t.Error("cache file contains the secret in plaintext")
	}
	// The vault and secret name are hashed, so the file does not disclose
	// which secrets are in use either.
	if strings.Contains(string(data), "conn") {
		t.Error("cache file discloses the secret name")
	}
}

func TestCacheFilePermissions(t *testing.T) {
	c := newCache(t, time.Minute)
	if err := c.Put(Ref{VaultHost: "v", Name: "n"}, "value"); err != nil {
		t.Fatal(err)
	}

	for _, name := range []string{"secret-cache.json", "cache.key"} {
		info, err := os.Stat(filepath.Join(c.Dir, name))
		if err != nil {
			t.Fatal(err)
		}
		if perm := info.Mode().Perm(); perm != 0o600 {
			t.Errorf("%s permissions = %o, want 600", name, perm)
		}
	}
}

func TestCacheExpiry(t *testing.T) {
	c := newCache(t, time.Nanosecond)
	ref := Ref{VaultHost: "v", Name: "n"}

	if err := c.Put(ref, "value"); err != nil {
		t.Fatal(err)
	}
	time.Sleep(time.Millisecond)

	if _, ok := c.Get(ref); ok {
		t.Error("an expired entry should miss")
	}
}

func TestCacheDisabledWritesNothing(t *testing.T) {
	c := newCache(t, 0)
	ref := Ref{VaultHost: "v", Name: "n"}

	if err := c.Put(ref, "value"); err != nil {
		t.Fatal(err)
	}
	if _, ok := c.Get(ref); ok {
		t.Error("a disabled cache must always miss")
	}
	if entries, _ := os.ReadDir(c.Dir); len(entries) != 0 {
		t.Errorf("a disabled cache must not touch the disk, found %d files", len(entries))
	}
}

// An entry must not be readable under a different reference, even if someone
// edits the file to move the ciphertext.
func TestCacheEntryIsBoundToItsReference(t *testing.T) {
	c := newCache(t, time.Minute)
	a := Ref{VaultHost: "v", Name: "a"}
	b := Ref{VaultHost: "v", Name: "b"}

	if err := c.Put(a, "secret-a"); err != nil {
		t.Fatal(err)
	}

	entry, err := c.encrypt(a, "secret-a")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := c.decrypt(b, entry); err == nil {
		t.Error("decrypting under the wrong reference should fail")
	}
}

func TestCacheClear(t *testing.T) {
	c := newCache(t, time.Minute)
	ref := Ref{VaultHost: "v", Name: "n"}

	if err := c.Put(ref, "value"); err != nil {
		t.Fatal(err)
	}
	if c.Count() != 1 {
		t.Errorf("Count() = %d, want 1", c.Count())
	}
	if err := c.Clear(); err != nil {
		t.Fatal(err)
	}
	if _, ok := c.Get(ref); ok {
		t.Error("cleared cache should miss")
	}
	if c.Count() != 0 {
		t.Errorf("Count() = %d after clear, want 0", c.Count())
	}
	// Clearing twice must not error.
	if err := c.Clear(); err != nil {
		t.Errorf("second Clear() failed: %v", err)
	}
}

func TestCacheCorruptFileIsNotFatal(t *testing.T) {
	c := newCache(t, time.Minute)
	if err := os.WriteFile(filepath.Join(c.Dir, "secret-cache.json"), []byte("{not json"), 0o600); err != nil {
		t.Fatal(err)
	}

	if _, ok := c.Get(Ref{VaultHost: "v", Name: "n"}); ok {
		t.Error("a corrupt cache should miss rather than return garbage")
	}
	// And a subsequent write should repair it.
	if err := c.Put(Ref{VaultHost: "v", Name: "n"}, "value"); err != nil {
		t.Fatalf("Put() on a corrupt cache failed: %v", err)
	}
	if _, ok := c.Get(Ref{VaultHost: "v", Name: "n"}); !ok {
		t.Error("cache should work after being repaired")
	}
}

// --- resolver ------------------------------------------------------------

type fakeProvider struct {
	value string
	err   error
	calls int
}

func (f *fakeProvider) Fetch(context.Context, Ref) (string, error) {
	f.calls++
	if f.err != nil {
		return "", f.err
	}
	return f.value, nil
}

func TestResolvePassesThroughLiterals(t *testing.T) {
	p := &fakeProvider{value: "unused"}
	r := &Resolver{Provider: p, Cache: newCache(t, time.Minute)}

	got, err := r.Resolve(context.Background(), "postgres://host/db")
	if err != nil {
		t.Fatal(err)
	}
	if got != "postgres://host/db" {
		t.Errorf("Resolve() = %q", got)
	}
	if p.calls != 0 {
		t.Errorf("a literal DSN must not contact the vault, got %d calls", p.calls)
	}
}

func TestResolveUsesCacheOnSecondCall(t *testing.T) {
	p := &fakeProvider{value: "postgres://host/db"}
	r := &Resolver{Provider: p, Cache: newCache(t, time.Minute)}

	for i := 0; i < 3; i++ {
		got, err := r.Resolve(context.Background(), "keyvault://v/conn")
		if err != nil {
			t.Fatal(err)
		}
		if got != "postgres://host/db" {
			t.Fatalf("Resolve() = %q", got)
		}
	}
	if p.calls != 1 {
		t.Errorf("provider called %d times, want 1 (the rest should be cached)", p.calls)
	}
}

func TestResolveWithoutCacheAlwaysFetches(t *testing.T) {
	p := &fakeProvider{value: "postgres://host/db"}
	r := &Resolver{Provider: p, Cache: newCache(t, 0)}

	for i := 0; i < 3; i++ {
		if _, err := r.Resolve(context.Background(), "keyvault://v/conn"); err != nil {
			t.Fatal(err)
		}
	}
	if p.calls != 3 {
		t.Errorf("provider called %d times, want 3", p.calls)
	}
}

func TestResolveRejectsEmptySecret(t *testing.T) {
	p := &fakeProvider{value: "   "}
	r := &Resolver{Provider: p, Cache: newCache(t, time.Minute)}

	if _, err := r.Resolve(context.Background(), "keyvault://v/conn"); err == nil {
		t.Error("an empty secret should be an error, not an empty DSN")
	}
}

func TestResolveErrorMentionsTheReferenceNotTheSecret(t *testing.T) {
	p := &fakeProvider{err: errors.New("SecretNotFound")}
	r := &Resolver{Provider: p, Cache: newCache(t, time.Minute)}

	_, err := r.Resolve(context.Background(), "keyvault://my-vault/conn")
	if err == nil {
		t.Fatal("expected an error")
	}
	if !strings.Contains(err.Error(), "my-vault") {
		t.Errorf("error should name the vault: %v", err)
	}
}

func TestExplainAuthAddsNextStep(t *testing.T) {
	err := explainAuth(errors.New("DefaultAzureCredential: failed to acquire a token"), "")
	if !strings.Contains(err.Error(), "az login") {
		t.Errorf("credential errors should suggest az login: %v", err)
	}
	if !strings.Contains(err.Error(), "BINSQL_AZURE_CREDENTIAL=cli") {
		t.Errorf("should suggest the faster credential: %v", err)
	}

	// ...but not when that is already how we authenticated.
	err = explainAuth(errors.New("AADSTS50020: no such user"), "cli")
	if strings.Contains(err.Error(), "BINSQL_AZURE_CREDENTIAL=cli") {
		t.Errorf("should not suggest a setting already in effect: %v", err)
	}

	err = explainAuth(errors.New("Forbidden"), "")
	if !strings.Contains(err.Error(), "Secrets User") {
		t.Errorf("permission errors should name the role: %v", err)
	}

	// An unrecognised error must pass through unchanged.
	orig := errors.New("connection reset")
	if got := explainAuth(orig, ""); got.Error() != orig.Error() {
		t.Errorf("unexpected rewrite: %v", got)
	}
}
