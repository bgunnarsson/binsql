package secrets

import (
	"context"
	"fmt"
	"strings"
	"sync"

	"github.com/Azure/azure-sdk-for-go/sdk/azcore"
	"github.com/Azure/azure-sdk-for-go/sdk/azidentity"
	"github.com/Azure/azure-sdk-for-go/sdk/security/keyvault/azsecrets"
)

// keyVaultProvider reads secrets from Azure Key Vault. By default it uses
// DefaultAzureCredential, which covers the ways these projects actually
// authenticate: `az login` on a developer machine, a managed identity on
// Azure, and AZURE_CLIENT_ID/SECRET in CI.
//
// The chain is convenient but not free: on a laptop it probes for a managed
// identity and waits for that to time out, which costs several seconds on
// every cache miss. Setting credential to "cli" skips straight to the Azure
// CLI and makes the fetch roughly immediate.
type keyVaultProvider struct {
	credential string

	mu   sync.Mutex
	cred azcore.TokenCredential
	// clients are cached per vault host; each one holds a connection pool.
	clients map[string]*azsecrets.Client
}

// newCredential builds the credential named by kind.
func newCredential(kind string) (azcore.TokenCredential, error) {
	switch strings.ToLower(strings.TrimSpace(kind)) {
	case "cli", "azurecli", "az":
		return azidentity.NewAzureCLICredential(nil)
	case "env", "environment", "sp", "serviceprincipal":
		return azidentity.NewEnvironmentCredential(nil)
	case "managed", "managedidentity", "msi":
		return azidentity.NewManagedIdentityCredential(nil)
	case "", "default", "chain":
		return azidentity.NewDefaultAzureCredential(nil)
	default:
		return nil, fmt.Errorf(
			"unknown Azure credential %q (expected default, cli, env or managed)", kind)
	}
}

func (p *keyVaultProvider) Fetch(ctx context.Context, ref Ref) (string, error) {
	client, err := p.clientFor(ref.VaultURL())
	if err != nil {
		return "", err
	}

	// An empty version means "current".
	resp, err := client.GetSecret(ctx, ref.Name, ref.Version, nil)
	if err != nil {
		return "", err
	}
	if resp.Value == nil {
		return "", fmt.Errorf("secret has no value")
	}
	return *resp.Value, nil
}

func (p *keyVaultProvider) clientFor(vaultURL string) (*azsecrets.Client, error) {
	p.mu.Lock()
	defer p.mu.Unlock()

	if client, ok := p.clients[vaultURL]; ok {
		return client, nil
	}

	if p.cred == nil {
		cred, err := newCredential(p.credential)
		if err != nil {
			return nil, err
		}
		p.cred = cred
	}

	client, err := azsecrets.NewClient(vaultURL, p.cred, nil)
	if err != nil {
		return nil, err
	}
	if p.clients == nil {
		p.clients = map[string]*azsecrets.Client{}
	}
	p.clients[vaultURL] = client
	return client, nil
}
