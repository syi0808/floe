package gmail

import (
	"context"
	"errors"
)

const MaxSyncItems = 500

type Syncer struct {
	client *Client
	index  *Index
	query  string
}

func NewSyncer(client *Client, index *Index, query string) (*Syncer, error) {
	if client == nil || index == nil || stringsTrimmedEmpty(query) || len(query) > 512 {
		return nil, ErrInvalidInput
	}
	return &Syncer{client: client, index: index, query: query}, nil
}

func (syncer *Syncer) Bootstrap(ctx context.Context) error {
	profile, err := syncer.client.Profile(ctx)
	if err != nil {
		return err
	}
	messages := []Metadata{}
	cursor := ""
	for {
		remaining := MaxSyncItems - len(messages)
		if remaining == 0 {
			return ErrInvalidResponse
		}
		page, err := syncer.client.Search(ctx, syncer.query, cursor, min(remaining, MaxPageItems))
		if err != nil {
			return err
		}
		for _, reference := range page.Messages {
			message, err := syncer.client.ReadMetadata(ctx, reference.ID)
			if err != nil {
				return err
			}
			messages = append(messages, message)
		}
		if page.NextCursor == "" {
			break
		}
		cursor = page.NextCursor
	}
	if err := syncer.index.ApplyFull(messages, profile.HistoryID); err != nil {
		return err
	}
	return syncer.incremental(ctx)
}

func (syncer *Syncer) Refresh(ctx context.Context) error {
	if syncer.index.HistoryID() == "" {
		return syncer.Bootstrap(ctx)
	}
	err := syncer.incremental(ctx)
	if errors.Is(err, ErrCheckpointExpired) {
		return syncer.Bootstrap(ctx)
	}
	return err
}

func (syncer *Syncer) incremental(ctx context.Context) error {
	previous := syncer.index.HistoryID()
	if !validID(previous) {
		return ErrInvalidInput
	}
	cursor := ""
	historyID := previous
	added := map[string]MessageRef{}
	deleted := map[string]bool{}
	for {
		page, err := syncer.client.Changes(ctx, previous, cursor, MaxPageItems)
		if err != nil {
			return err
		}
		for _, reference := range page.Added {
			added[reference.ID] = reference
		}
		for _, reference := range page.Changed {
			added[reference.ID] = reference
		}
		for _, reference := range page.Deleted {
			deleted[reference.ID] = true
		}
		if len(added)+len(deleted) > MaxSyncItems {
			return ErrInvalidResponse
		}
		historyID = page.HistoryID
		if page.NextCursor == "" {
			break
		}
		cursor = page.NextCursor
	}
	upserts := make([]Metadata, 0, len(added))
	for id := range added {
		if deleted[id] {
			continue
		}
		message, err := syncer.client.ReadMetadata(ctx, id)
		if err != nil {
			return err
		}
		upserts = append(upserts, message)
	}
	deletedIDs := make([]string, 0, len(deleted))
	for id := range deleted {
		deletedIDs = append(deletedIDs, id)
	}
	return syncer.index.ApplyDelta(upserts, deletedIDs, previous, historyID)
}

func stringsTrimmedEmpty(value string) bool {
	for _, character := range value {
		if character != ' ' && character != '\t' && character != '\r' && character != '\n' {
			return false
		}
	}
	return true
}
