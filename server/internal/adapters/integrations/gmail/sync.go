package gmail

import (
	"context"
	"strings"
)

const MaxSyncItems = 500

type Syncer struct {
	client *Client
	index  *Index
	query  string
}

func NewSyncer(client *Client, index *Index, query string) (*Syncer, error) {
	query = strings.TrimSpace(query)
	if client == nil || index == nil || query == "" || len(query) > 512 {
		return nil, ErrInvalidInput
	}
	return &Syncer{client: client, index: index, query: query}, nil
}

// Refresh obtains query membership anew, including relative-time expiry and
// label changes without treating mailbox-wide history as query authorization.
// The old encrypted snapshot is replaced only after the bounded fetch succeeds.
func (syncer *Syncer) Refresh(ctx context.Context) error {
	if ctx == nil {
		return ErrInvalidInput
	}
	messages := make([]Metadata, 0)
	seen := map[string]bool{}
	cursors := map[string]bool{}
	cursor := ""
	for {
		if err := ctx.Err(); err != nil {
			return err
		}
		remaining := MaxSyncItems - len(messages)
		if remaining == 0 || cursors[cursor] {
			return ErrInvalidResponse
		}
		cursors[cursor] = true
		page, err := syncer.client.Search(ctx, syncer.query, cursor, min(remaining, MaxPageItems))
		if err != nil {
			return err
		}
		if len(page.Messages) == 0 && page.NextCursor != "" {
			return ErrInvalidResponse
		}
		for _, ref := range page.Messages {
			if seen[ref.ID] {
				return ErrInvalidResponse
			}
			seen[ref.ID] = true
			message, err := syncer.client.ReadMetadata(ctx, ref.ID)
			if err != nil {
				return err
			}
			if message.ID != ref.ID || message.ThreadID != ref.ThreadID {
				return ErrInvalidResponse
			}
			messages = append(messages, message)
		}
		if page.NextCursor == "" {
			break
		}
		cursor = page.NextCursor
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	return syncer.index.ReplaceQuerySnapshot(messages, syncer.query)
}
