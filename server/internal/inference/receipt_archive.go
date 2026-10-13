package inference

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
)

const MaxConfigReceiptPageBytes = 32_768

// The live snapshot atomically publishes the immutable history head. Missing
// referenced pages are storage failures, never evidence of command absence.
type ConfigReceiptArchive struct {
	Head  string `json:"head"`
	Pages uint64 `json:"pages"`
}

func (a ConfigReceiptArchive) valid() bool {
	return a.Pages == 0 && a.Head == "" || a.Pages > 0 && validDigest(a.Head)
}

type ConfigReceiptPage struct {
	Previous string                 `json:"previous"`
	Sequence uint64                 `json:"sequence"`
	Receipts []ConfigurationReceipt `json:"receipts"`
}
type ConfigReceiptPageRead struct {
	Disposition ConfigReadDisposition
	Page        ConfigReceiptPage
	Cause       error
}

// ConfigReceiptPageDigest validates the bounded owner contract and addresses its
// canonical encoding. Hash links authenticate both positive lookup and absence.
func ConfigReceiptPageDigest(page ConfigReceiptPage) (string, error) {
	if len(page.Receipts) != maxConfigurationReceipts || page.Sequence == 0 ||
		page.Sequence == 1 && page.Previous != "" || page.Sequence > 1 && !validDigest(page.Previous) {
		return "", errors.New("invalid configuration receipt page")
	}
	seen := make(map[string]bool, len(page.Receipts))
	for _, r := range page.Receipts {
		if !validConfigurationReceipt(r) || seen[r.OperationID] {
			return "", errors.New("invalid configuration receipt")
		}
		seen[r.OperationID] = true
	}
	data, err := json.Marshal(page)
	if err != nil || len(data) > MaxConfigReceiptPageBytes {
		return "", errors.New("configuration receipt page capacity")
	}
	digest := sha256.Sum256(data)
	return hex.EncodeToString(digest[:]), nil
}

// archiveReceipts reserves one hot receipt slot before a new configuration or
// credential intent is committed. An orphan immutable page after rejection is
// harmless: only a committed snapshot head can publish it as command evidence.
func (c *Configuration) archiveReceipts(ctx context.Context, next *ConfigState) error {
	if len(next.Receipts) < maxConfigurationReceipts {
		return nil
	}
	if ctx.Err() != nil || next.ReceiptArchive.Pages == ^uint64(0) {
		return errors.New("receipt archive unavailable")
	}
	page := ConfigReceiptPage{Previous: next.ReceiptArchive.Head, Sequence: next.ReceiptArchive.Pages + 1, Receipts: append([]ConfigurationReceipt(nil), next.Receipts...)}
	digest, err := ConfigReceiptPageDigest(page)
	if err != nil {
		return err
	}
	if c.repository.SaveReceiptPage(digest, page).Disposition != ConfigWriteCommitted {
		return errors.New("receipt archive unavailable")
	}
	next.ReceiptArchive = ConfigReceiptArchive{Head: digest, Pages: page.Sequence}
	next.Receipts = []ConfigurationReceipt{}
	return nil
}

func (c *Configuration) findReceipt(ctx context.Context, operationID string) (*ConfigurationReceipt, error) {
	for _, receipt := range c.state.Receipts {
		if receipt.OperationID == operationID {
			return &receipt, nil
		}
	}
	head := c.state.ReceiptArchive.Head
	for sequence := c.state.ReceiptArchive.Pages; sequence > 0; sequence-- {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		read := c.repository.LoadReceiptPage(head)
		if read.Disposition != ConfigReadPresent {
			return nil, errors.New("referenced receipt page unavailable")
		}
		digest, err := ConfigReceiptPageDigest(read.Page)
		if err != nil || digest != head || read.Page.Sequence != sequence {
			return nil, errors.New("invalid receipt archive chain")
		}
		for _, receipt := range read.Page.Receipts {
			if receipt.OperationID == operationID {
				return &receipt, nil
			}
		}
		head = read.Page.Previous
	}
	if head != "" {
		return nil, errors.New("invalid receipt archive tail")
	}
	return nil, nil
}
