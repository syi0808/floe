// Package node assembles semantic owners and real provider adapters.
package node

import (
	"context"
	"errors"
	credentialadapter "floe/server/internal/adapters/credentials"
	codexauth "floe/server/internal/adapters/models/codex"
	"floe/server/internal/adapters/models/providers"
	storageadapter "floe/server/internal/adapters/storage"
	"floe/server/internal/authority"
	"floe/server/internal/inference"
	"floe/server/internal/integrations"
	"floe/server/internal/modelcatalog"
	"floe/server/internal/pairing"
	httptransport "floe/server/internal/transport/http"
	"floe/server/internal/trust"
	"floe/server/internal/views"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"sync"
	"time"
)

type Config struct {
	Directory                   string
	Address                     string
	ModelCatalogRefreshInterval time.Duration
}
type Node struct {
	handler      *httptransport.Handler
	integrations *integrations.Service
	clients      *trust.ClientAdministration
	runtime      *codexauth.Runtime
	storage      *admittedStorage
	catalogDone  chan struct{}
	close        sync.Once
	mu           sync.Mutex
	active       sync.WaitGroup
	closed       bool
	ctx          context.Context
	cancel       context.CancelFunc
}

func New(config Config) (*Node, error) {
	refreshInterval := config.ModelCatalogRefreshInterval
	if refreshInterval == 0 {
		refreshInterval = modelcatalog.DefaultRefreshInterval
	}
	if refreshInterval < modelcatalog.MinRefreshInterval || refreshInterval > modelcatalog.MaxRefreshInterval {
		return nil, errors.New("model catalog refresh interval out of bounds")
	}
	host, port, err := net.SplitHostPort(config.Address)
	if err != nil || host != "127.0.0.1" || port == "" {
		return nil, errors.New("node requires 127.0.0.1:port")
	}
	vault, err := profileStore(config.Directory, true)
	if err != nil {
		return nil, err
	}
	storageRoot, err := openStorage(context.Background(), config.Directory, vault, true)
	if err != nil {
		return nil, err
	}
	failed := true
	var viewService *views.Service
	var mirrorService *views.CalendarMirrorService
	defer func() {
		if failed {
			storageRoot.Close()
		}
	}()
	modelCatalog, err := modelcatalog.Open(filepath.Join(config.Directory, modelcatalog.FileName))
	if err != nil {
		return nil, err
	}
	trustFiles, err := storageRoot.files.Scope("trust")
	if err != nil {
		return nil, startupStorageFailure(err, "owner_storage_unavailable", stageOwnerStorage, storageRoot.fresh)
	}
	inferenceFiles, err := storageRoot.files.Scope("inference")
	if err != nil {
		return nil, startupStorageFailure(err, "owner_storage_unavailable", stageOwnerStorage, storageRoot.fresh)
	}
	integrationFiles, err := storageRoot.files.Scope("integrations")
	if err != nil {
		return nil, startupStorageFailure(err, "owner_storage_unavailable", stageOwnerStorage, storageRoot.fresh)
	}
	connectorFiles, err := storageRoot.files.Scope("connectors")
	if err != nil {
		return nil, startupStorageFailure(err, "owner_storage_unavailable", stageOwnerStorage, storageRoot.fresh)
	}
	trustRepository := storageadapter.NewTrustRepository(trustFiles)
	t, err := trust.Open(trustRepository, storageRoot.fresh)
	if err != nil {
		return nil, startupStorageFailure(err, "trust_unavailable", stageTrustStorage, storageRoot.fresh)
	}
	if err := storageRoot.publishReady(); err != nil {
		return nil, err
	}
	runtime := codexauth.New(vault)
	defer func() {
		if failed {
			runtime.Close()
		}
	}()
	model, err := inference.NewService(t)
	if err != nil {
		return nil, err
	}
	providerCredentials := credentialadapter.NewInferenceProviderAccess(vault)
	factory := providers.NewFactory(providerCredentials.ReadProviderCredential, runtime)
	configurationRepository := storageadapter.NewInferenceConfigRepository(inferenceFiles)
	configuration, err := inference.OpenConfiguration(context.Background(), configurationRepository, model, t, providerCredentials, factory)
	if err != nil {
		return nil, err
	}
	engine, err := authority.New(authority.Options{Trust: t})
	if err != nil {
		return nil, err
	}
	integrationRepository := storageadapter.NewIntegrationsRepository(integrationFiles)
	integrationCredentials := credentialadapter.NewIntegrationAccess(vault)
	sources, err := integrations.New(context.Background(), integrationRepository, t, integrationCredentials, integrationFactories(connectorFiles, vault, os.Getenv))
	if err != nil {
		return nil, err
	}
	defer func() {
		if failed {
			if mirrorService != nil {
				mirrorService.Close()
			}
			if viewService != nil {
				viewService.Close()
			}
			sources.Close()
		}
	}()
	enforcement, err := authority.NewViewEnforcer(engine, t, sources)
	if err != nil {
		return nil, err
	}
	reader, err := views.NewService(enforcement, t, sources)
	if err != nil {
		return nil, err
	}
	viewService = reader
	mirrorEnforcement, err := authority.NewCalendarMirrorEnforcer(engine, t, sources)
	if err != nil {
		return nil, err
	}
	mirror, err := views.NewCalendarMirrorService(mirrorEnforcement, t, sources)
	if err != nil {
		return nil, err
	}
	mirrorService = mirror
	pairingCredentials := credentialadapter.NewPairingAccess(vault)
	pairing := pairing.NewOperations(t, pairingCredentials, nil)
	clients := trust.NewClientAdministration(t, sources, pairing)
	handler := &httptransport.Handler{Address: config.Address, Trust: t, Pairing: pairing, Setup: sources, Integrations: sources, Sources: reader, Mirror: mirror, Configuration: configuration, Accounts: inference.NewAccountManagement(t, runtime), Clients: clients, ModelCatalog: modelCatalog, Inference: &httptransport.InferenceHandler{Service: model, Trust: t, ModelCatalog: modelCatalog, Address: config.Address}}
	ctx, cancel := context.WithCancel(context.Background())
	catalogDone := make(chan struct{})
	go func() {
		defer close(catalogDone)
		_ = modelCatalog.Run(ctx, refreshInterval)
	}()
	failed = false
	return &Node{handler: handler, integrations: sources, clients: clients, runtime: runtime, storage: storageRoot, catalogDone: catalogDone, ctx: ctx, cancel: cancel}, nil
}
func (n *Node) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	n.mu.Lock()
	if n.closed || n.storage.files.Available() != nil {
		n.mu.Unlock()
		n.handler.ServeUnavailable(w)
		return
	}
	n.active.Add(1)
	n.mu.Unlock()
	defer n.active.Done()
	ctx, cancel := context.WithCancel(r.Context())
	stop := context.AfterFunc(n.ctx, cancel)
	defer stop()
	defer cancel()
	n.handler.ServeHTTP(w, r.WithContext(ctx))
}
func (n *Node) Close() {
	n.close.Do(func() {
		n.mu.Lock()
		n.closed = true
		n.cancel()
		n.mu.Unlock()
		if n.catalogDone != nil {
			<-n.catalogDone
		}
		n.handler.Inference.Service.DenyConfiguration()
		n.active.Wait()
		n.handler.Configuration.Close()
		n.handler.Mirror.Close()
		n.handler.Sources.Close()
		n.clients.Close()
		n.integrations.Close()
		n.runtime.Close()
		n.storage.Close()
	})
}
