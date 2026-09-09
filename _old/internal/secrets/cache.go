package secrets

import (
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"time"
)

// Cache stores resolved secrets on disk so a run of several commands does not
// pay the vault round-trip every time.
//
// Entries are encrypted with AES-256-GCM under a key kept beside the cache,
// and both files are written 0600. Be clear about the threat model: because
// the key sits next to the ciphertext, this protects against a secret being
// scooped up incidentally — by a backup, a directory sync, a shared screen,
// or a grep across the home directory — not against someone who can already
// read your files as you. It is a meaningful reduction in accidental
// exposure, not a vault of its own. Set the TTL to 0 to keep secrets off the
// disk entirely.
type Cache struct {
	Dir string
	TTL time.Duration
}

type cacheFile struct {
	Version int                   `json:"version"`
	Entries map[string]cacheEntry `json:"entries"`
}

type cacheEntry struct {
	Nonce     string    `json:"nonce"`
	Value     string    `json:"value"`
	ExpiresAt time.Time `json:"expires_at"`
}

const cacheVersion = 1

func (c *Cache) cachePath() string { return filepath.Join(c.Dir, "secret-cache.json") }
func (c *Cache) keyPath() string   { return filepath.Join(c.Dir, "cache.key") }

// enabled reports whether caching is switched on.
func (c *Cache) enabled() bool { return c != nil && c.TTL > 0 && c.Dir != "" }

// Get returns a cached secret, or ok=false when absent, expired or unreadable.
// A damaged cache is never fatal: the caller simply refetches.
func (c *Cache) Get(ref Ref) (string, bool) {
	if !c.enabled() {
		return "", false
	}

	file, err := c.load()
	if err != nil {
		return "", false
	}
	entry, ok := file.Entries[cacheKey(ref)]
	if !ok || time.Now().After(entry.ExpiresAt) {
		return "", false
	}

	value, err := c.decrypt(ref, entry)
	if err != nil {
		return "", false
	}
	return value, true
}

// Put stores a secret until the TTL elapses.
func (c *Cache) Put(ref Ref, value string) error {
	if !c.enabled() {
		return nil
	}
	if err := os.MkdirAll(c.Dir, 0o700); err != nil {
		return err
	}

	entry, err := c.encrypt(ref, value)
	if err != nil {
		return err
	}

	file, err := c.load()
	if err != nil || file.Entries == nil {
		file = &cacheFile{Version: cacheVersion, Entries: map[string]cacheEntry{}}
	}
	file.Entries[cacheKey(ref)] = entry
	file.prune()

	return c.save(file)
}

// Clear removes every cached secret and the key that protected them.
func (c *Cache) Clear() error {
	if c == nil || c.Dir == "" {
		return nil
	}
	for _, p := range []string{c.cachePath(), c.keyPath()} {
		if err := os.Remove(p); err != nil && !os.IsNotExist(err) {
			return err
		}
	}
	return nil
}

// Count reports how many unexpired entries are cached, for `conn cache`.
func (c *Cache) Count() int {
	file, err := c.load()
	if err != nil {
		return 0
	}
	n := 0
	now := time.Now()
	for _, e := range file.Entries {
		if now.Before(e.ExpiresAt) {
			n++
		}
	}
	return n
}

// prune drops expired entries so the file does not grow without bound.
func (f *cacheFile) prune() {
	now := time.Now()
	for k, e := range f.Entries {
		if now.After(e.ExpiresAt) {
			delete(f.Entries, k)
		}
	}
}

func (c *Cache) load() (*cacheFile, error) {
	data, err := os.ReadFile(c.cachePath())
	if err != nil {
		return nil, err
	}
	var file cacheFile
	if err := json.Unmarshal(data, &file); err != nil {
		return nil, err
	}
	if file.Version != cacheVersion || file.Entries == nil {
		return nil, fmt.Errorf("unsupported cache version %d", file.Version)
	}
	return &file, nil
}

func (c *Cache) save(file *cacheFile) error {
	data, err := json.MarshalIndent(file, "", "  ")
	if err != nil {
		return err
	}
	data = append(data, '\n')

	tmp := c.cachePath() + ".tmp"
	if err := os.WriteFile(tmp, data, 0o600); err != nil {
		return err
	}
	return os.Rename(tmp, c.cachePath())
}

// key loads the local encryption key, generating one on first use.
func (c *Cache) key() ([]byte, error) {
	data, err := os.ReadFile(c.keyPath())
	if err == nil {
		key, decErr := base64.StdEncoding.DecodeString(string(data))
		if decErr == nil && len(key) == 32 {
			return key, nil
		}
		// A corrupt key file means the existing entries are unreadable;
		// replacing it simply forces a refetch.
	} else if !os.IsNotExist(err) {
		return nil, err
	}

	key := make([]byte, 32)
	if _, err := rand.Read(key); err != nil {
		return nil, err
	}
	if err := os.MkdirAll(c.Dir, 0o700); err != nil {
		return nil, err
	}
	encoded := []byte(base64.StdEncoding.EncodeToString(key))
	if err := os.WriteFile(c.keyPath(), encoded, 0o600); err != nil {
		return nil, err
	}
	return key, nil
}

func (c *Cache) aead() (cipher.AEAD, error) {
	key, err := c.key()
	if err != nil {
		return nil, err
	}
	block, err := aes.NewCipher(key)
	if err != nil {
		return nil, err
	}
	return cipher.NewGCM(block)
}

func (c *Cache) encrypt(ref Ref, value string) (cacheEntry, error) {
	gcm, err := c.aead()
	if err != nil {
		return cacheEntry{}, err
	}

	nonce := make([]byte, gcm.NonceSize())
	if _, err := rand.Read(nonce); err != nil {
		return cacheEntry{}, err
	}

	// Bind the ciphertext to its reference so an entry cannot be swapped
	// for another vault's secret by editing the file.
	sealed := gcm.Seal(nil, nonce, []byte(value), []byte(ref.String()))

	return cacheEntry{
		Nonce:     base64.StdEncoding.EncodeToString(nonce),
		Value:     base64.StdEncoding.EncodeToString(sealed),
		ExpiresAt: time.Now().Add(c.TTL),
	}, nil
}

func (c *Cache) decrypt(ref Ref, entry cacheEntry) (string, error) {
	gcm, err := c.aead()
	if err != nil {
		return "", err
	}
	nonce, err := base64.StdEncoding.DecodeString(entry.Nonce)
	if err != nil {
		return "", err
	}
	sealed, err := base64.StdEncoding.DecodeString(entry.Value)
	if err != nil {
		return "", err
	}
	if len(nonce) != gcm.NonceSize() {
		return "", fmt.Errorf("bad nonce length")
	}

	plain, err := gcm.Open(nil, nonce, sealed, []byte(ref.String()))
	if err != nil {
		return "", err
	}
	return string(plain), nil
}

// cacheKey hashes the reference, so the file does not disclose which vaults
// and secret names are in use.
func cacheKey(ref Ref) string {
	sum := sha256.Sum256([]byte(ref.String()))
	return hex.EncodeToString(sum[:])
}
