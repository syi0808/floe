package application

import (
	"errors"
	"fmt"
	"net/http"

	"floe/server/internal/console"
)

var ErrRequiredDependency = errors.New("required application dependency unavailable")
var ErrRequiredSecurity = errors.New("required application security unavailable")

type LocalConfig struct {
	Directory string
	Address   string
	Vault     console.Vault
	Runtime   console.AuthRuntime
}

type LocalServer struct {
	Management *console.Console
}

func NewLocal(config LocalConfig) (*LocalServer, error) {
	if config.Vault == nil || config.Runtime == nil {
		return nil, ErrRequiredDependency
	}
	management, err := console.New(config.Directory, config.Address, config.Vault, config.Runtime)
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrRequiredSecurity, err)
	}
	return &LocalServer{Management: management}, nil
}

func (server *LocalServer) ServeHTTP(writer http.ResponseWriter, request *http.Request) {
	server.Management.ServeHTTP(writer, request)
}

type ModuleStatus struct {
	Name       string
	Available  bool
	Diagnostic string
}

func ConfigureOptional(name string, initialize func() error) ModuleStatus {
	status := ModuleStatus{Name: name, Diagnostic: "optional module unavailable"}
	if name == "" || initialize == nil {
		return status
	}
	if initialize() != nil {
		return status
	}
	status.Available, status.Diagnostic = true, "available"
	return status
}
