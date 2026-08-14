package secrets

import (
	"context"
	"fmt"
	"os"
	"strings"
	"time"
)

// DefaultTTL is how long a resolved secret stays cached.
const DefaultTTL = 15 * time.Minute

// Provider fetches a secret value from a vault.
type Provider interface {
	Fetch(ctx context.Context, ref Ref) (string, error)
}

// Resolver turns a reference into a connection string, consulting the cache
// before the vault.
type Resolver struct {
	Provider Provider
	Cache    *Cache
	Suffix   string
	// Credential is recorded only so error messages can avoid suggesting a
	// setting that is already in effect.
	Credential string
}

// Options configures a Resolver from the CLI's settings.
type Options struct {
	// Dir is where the cache lives; usually the binsql config directory.
	Dir string
	// TTL of 0 disables caching entirely.
	TTL time.Duration
	// Suffix overrides the Key Vault DNS suffix for sovereign clouds.
	Suffix string
	// Credential selects how to authenticate: "default" (the full chain),
	// "cli", "env" or "managed". Empty means default.
	Credential string
}

// New builds a Resolver backed by Azure Key Vault.
func New(opts Options) *Resolver {
	suffix := opts.Suffix
	if suffix == "" {
		suffix = os.Getenv("BINSQL_KEYVAULT_SUFFIX")
	}
	credential := opts.Credential
	if credential == "" {
		credential = os.Getenv("BINSQL_AZURE_CREDENTIAL")
	}

	return &Resolver{
		Provider:   &keyVaultProvider{credential: credential},
		Cache:      &Cache{Dir: opts.Dir, TTL: opts.TTL},
		Suffix:     suffix,
		Credential: credential,
	}
}

// Resolve returns dsn unchanged when it is a literal connection string, or
// fetches the referenced secret when it is a reference.
func (r *Resolver) Resolve(ctx context.Context, dsn string) (string, error) {
	if !IsRef(dsn) {
		return dsn, nil
	}

	ref, err := Parse(dsn, r.Suffix)
	if err != nil {
		return "", err
	}

	if value, ok := r.Cache.Get(ref); ok {
		return value, nil
	}

	value, err := r.Provider.Fetch(ctx, ref)
	if err != nil {
		return "", fmt.Errorf("reading %s: %w", ref, explainAuth(err, r.Credential))
	}
	if strings.TrimSpace(value) == "" {
		return "", fmt.Errorf("secret %s is empty", ref)
	}

	// A cache write failure must not break an otherwise successful command.
	if err := r.Cache.Put(ref, value); err != nil {
		fmt.Fprintf(os.Stderr, "warning: could not cache secret: %v\n", err)
	}
	return value, nil
}

// explainAuth turns the SDK's opaque credential errors into a next step.
// Getting told to run `az login` beats decoding a chained-credential dump.
func explainAuth(err error, credential string) error {
	msg := err.Error()
	switch {
	case strings.Contains(msg, "DefaultAzureCredential"),
		strings.Contains(msg, "failed to acquire a token"),
		strings.Contains(msg, "AADSTS"):
		hint := "\n\nno usable Azure credential was found — run `az login`, " +
			"or set AZURE_CLIENT_ID / AZURE_TENANT_ID / AZURE_CLIENT_SECRET for a service principal."
		// Only worth suggesting when it is not already what we did.
		if !usingCLI(credential) {
			hint += "\nIf you authenticate with the Azure CLI, set BINSQL_AZURE_CREDENTIAL=cli " +
				"to skip the managed-identity probe and its several-second timeout."
		}
		return fmt.Errorf("%w%s", err, hint)
	case strings.Contains(msg, "Forbidden"), strings.Contains(msg, "does not have secrets get permission"):
		return fmt.Errorf("%w\n\nthe signed-in identity lacks 'get' on this secret — "+
			"grant it the Key Vault Secrets User role", err)
	case strings.Contains(msg, "SecretNotFound"), strings.Contains(msg, "was not found"):
		return fmt.Errorf("%w\n\ncheck the secret name and vault", err)
	default:
		return err
	}
}

func usingCLI(credential string) bool {
	switch strings.ToLower(strings.TrimSpace(credential)) {
	case "cli", "azurecli", "az":
		return true
	default:
		return false
	}
}
