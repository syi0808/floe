package homeassistant

import (
	"context"
	"time"
)

type Service struct {
	client   *Client
	entities []string
	clock    func() time.Time
}

func NewService(client *Client, entities []string) (*Service, error) {
	if client == nil || len(entities) == 0 || len(entities) > maxEntities {
		return nil, ErrInvalidInput
	}
	selected := append([]string(nil), entities...)
	seen := map[string]bool{}
	for _, entity := range selected {
		if !allowedEntity.MatchString(entity) || seen[entity] {
			return nil, ErrInvalidInput
		}
		seen[entity] = true
	}
	return &Service{client: client, entities: selected, clock: time.Now}, nil
}

func (service *Service) ReadLogisticsView(ctx context.Context) (any, error) {
	return service.client.Logistics(ctx, service.entities, service.clock())
}

func (service *Service) ConnectionSnapshot(ctx context.Context) (any, error) {
	view, err := service.client.Logistics(ctx, service.entities, service.clock())
	if err != nil {
		return nil, err
	}
	return ConnectionSnapshot(view)
}
