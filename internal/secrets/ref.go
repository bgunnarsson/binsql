// Package secrets resolves connection strings that are stored outside the
// binsql config — currently Azure Key Vault — so a profile can reference a
// secret instead of holding one.
package secrets

import (
	"fmt"
	"net/url"
	"strings"
)

// Ref points at a secret in a vault. The zero value means "not a reference";
// use Parse to build one.
type Ref struct {
	// VaultHost is the fully-qualified vault host, e.g.
	// "my-vault.vault.azure.net".
	VaultHost string
	// Name is the secret name within the vault.
	Name string
	// Version is optional; empty means the current version.
	Version string
}

// defaultSuffix is the Key Vault DNS suffix for the public Azure cloud.
// Override with BINSQL_KEYVAULT_SUFFIX for sovereign clouds.
const defaultSuffix = "vault.azure.net"

// IsRef reports whether a DSN is a secret reference rather than a literal
// connection string. It is cheap and does no I/O, so callers can branch on it
// before deciding to authenticate.
func IsRef(dsn string) bool {
	lower := strings.ToLower(strings.TrimSpace(dsn))
	if strings.HasPrefix(lower, "keyvault://") || strings.HasPrefix(lower, "azkv://") {
		return true
	}
	// The canonical secret identifier copied from the Azure portal.
	return strings.HasPrefix(lower, "https://") && strings.Contains(lower, "/secrets/")
}

// Parse turns a reference into its parts. Accepted forms:
//
//	keyvault://my-vault/secret-name
//	keyvault://my-vault/secret-name/version
//	keyvault://my-vault.vault.azure.net/secret-name
//	https://my-vault.vault.azure.net/secrets/secret-name[/version]
func Parse(dsn string, suffix string) (Ref, error) {
	raw := strings.TrimSpace(dsn)
	if suffix == "" {
		suffix = defaultSuffix
	}

	u, err := url.Parse(raw)
	if err != nil {
		return Ref{}, fmt.Errorf("invalid secret reference %q: %w", raw, err)
	}

	switch strings.ToLower(u.Scheme) {
	case "keyvault", "azkv":
		host := u.Host
		if host == "" {
			return Ref{}, fmt.Errorf("secret reference %q is missing a vault name", raw)
		}
		// A bare name gets the cloud's DNS suffix; a dotted name is a host.
		if !strings.Contains(host, ".") {
			host = host + "." + suffix
		}

		parts := splitPath(u.Path)
		switch len(parts) {
		case 1:
			return Ref{VaultHost: host, Name: parts[0]}, nil
		case 2:
			return Ref{VaultHost: host, Name: parts[0], Version: parts[1]}, nil
		default:
			return Ref{}, fmt.Errorf(
				"secret reference %q should look like keyvault://<vault>/<secret>[/<version>]", raw)
		}

	case "https":
		parts := splitPath(u.Path)
		if len(parts) < 2 || !strings.EqualFold(parts[0], "secrets") {
			return Ref{}, fmt.Errorf(
				"secret identifier %q should look like https://<vault-host>/secrets/<secret>[/<version>]", raw)
		}
		ref := Ref{VaultHost: u.Host, Name: parts[1]}
		if len(parts) > 2 {
			ref.Version = parts[2]
		}
		return ref, nil

	default:
		return Ref{}, fmt.Errorf("%q is not a secret reference", raw)
	}
}

// VaultURL is the endpoint the Key Vault client connects to.
func (r Ref) VaultURL() string { return "https://" + r.VaultHost + "/" }

// String renders the reference in its canonical form. It never contains the
// secret value, so it is safe to log.
func (r Ref) String() string {
	s := "keyvault://" + r.VaultHost + "/" + r.Name
	if r.Version != "" {
		s += "/" + r.Version
	}
	return s
}

func splitPath(p string) []string {
	var out []string
	for _, seg := range strings.Split(strings.Trim(p, "/"), "/") {
		if seg != "" {
			out = append(out, seg)
		}
	}
	return out
}
