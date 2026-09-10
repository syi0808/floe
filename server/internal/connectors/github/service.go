package github

import (
	"context"
	"time"
)

type Service struct {
	client     *Client
	owner      string
	repository string
	clock      func() time.Time
}

func NewService(client *Client, owner, repository string) (*Service, error) {
	if client == nil || !identifier.MatchString(owner) || !identifier.MatchString(repository) {
		return nil, ErrInvalidInput
	}
	return &Service{client: client, owner: owner, repository: repository, clock: time.Now}, nil
}

func (service *Service) ReadWorkContextView(ctx context.Context) (any, error) {
	return service.client.WorkContext(ctx, service.owner, service.repository, service.clock())
}

func (service *Service) ConnectionSnapshot(ctx context.Context) (any, error) {
	view, err := service.client.WorkContext(ctx, service.owner, service.repository, service.clock())
	if err != nil {
		return nil, err
	}
	return ConnectionSnapshot(view)
}
