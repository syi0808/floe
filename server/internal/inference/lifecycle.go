package inference

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"reflect"
	"strings"
	"time"

	"floe/server/internal/operation"
	"floe/server/internal/trust"
)

func operationFingerprint(value any) string {
	data, _ := json.Marshal(value)
	sum := sha256.Sum256(data)
	return hex.EncodeToString(sum[:])
}

func credentialSlot() string {
	return "FLOE_KEY_" + strings.ToUpper(strings.ReplaceAll(trust.NewID(), "-", ""))
}

func (c *Configuration) freshCredentialSlot(ctx context.Context) (string, error) {
	for attempt := 0; attempt < 8; attempt++ {
		slot := credentialSlot()
		value, err := c.credentials.ReadProviderCredential(ctx, slot)
		if err != nil {
			return "", err
		}
		if value == "" {
			return slot, nil
		}
	}
	return "", errors.New("credential reference collision")
}

func (c *Configuration) operationStatus(ctx context.Context, operationID, fingerprint string) (operation.Result, bool) {
	for _, receipt := range c.state.Receipts {
		if receipt.OperationID != operationID {
			continue
		}
		if receipt.Fingerprint != fingerprint {
			return operation.Reject(operation.Conflict, "operation_id_reused"), true
		}
		if c.configUnavailable {
			return operation.Reject(operation.Unavailable, "configuration_unavailable"), true
		}
		if receipt.Category == string(operation.Ready) {
			return operation.Accept(map[string]bool{"ok": true}), true
		}
		return operation.Reject(operation.Category(receipt.Category), receipt.Code), true
	}
	if pending := c.state.Pending; pending != nil {
		if pending.OperationID != operationID || pending.Fingerprint != fingerprint {
			return operation.Reject(operation.Conflict, "configuration_transition_pending"), true
		}
		if c.configUnavailable {
			return operation.Reject(operation.Unavailable, "configuration_unavailable"), true
		}
		if err := c.resolvePending(ctx, false); err != nil {
			return operation.Reject(operation.Unavailable, "configuration_unavailable"), true
		}
		for _, receipt := range c.state.Receipts {
			if receipt.OperationID == operationID && receipt.Fingerprint == fingerprint {
				if receipt.Category == string(operation.Ready) {
					return operation.Accept(map[string]bool{"ok": true}), true
				}
				return operation.Reject(operation.Category(receipt.Category), receipt.Code), true
			}
		}
		return operation.Reject(operation.Unavailable, "configuration_transition_pending"), true
	}
	return operation.Result{}, false
}

// commitOperation is called with c.mu held. It durably stores a candidate and
// bounded receipt before asking the live engine to adopt it.
func (c *Configuration) commitOperation(ctx context.Context, operationID, fingerprint string, content ConfigContent) operation.Result {
	if !trust.ValidID(operationID) || !validDigest(fingerprint) {
		return operation.Reject(operation.Invalid, "validation")
	}
	if result, found := c.operationStatus(ctx, operationID, fingerprint); found {
		return result
	}
	if c.configUnavailable {
		return operation.Reject(operation.Unavailable, "configuration_unavailable")
	}
	if len(c.state.Receipts) >= maxConfigurationReceipts {
		return operation.Reject(operation.Limited, "configuration_receipt_capacity")
	}
	if c.state.Revision == ^uint64(0) {
		return operation.Reject(operation.Limited, "configuration_revision_capacity")
	}
	next := cloneConfigurationState(c.state)
	next = withConfigurationContent(next, cloneConfigContent(content))
	next.Revision++
	next.Receipts = append(next.Receipts, ConfigurationReceipt{OperationID: operationID, Fingerprint: fingerprint, Category: string(operation.Ready), Code: "ok"})
	if err := c.planRetiredSlots(&next, c.state, c.engine.Generation()); err != nil {
		return operation.Reject(operation.Limited, "credential_cleanup_capacity")
	}
	return c.commitAndAdopt(ctx, next)
}

func (c *Configuration) commitAndAdopt(ctx context.Context, state ConfigState) operation.Result {
	if !configurationStateFitsBound(state) {
		return operation.Reject(operation.Limited, "configuration_snapshot_capacity")
	}
	if ctx.Err() != nil {
		return operation.Reject(operation.Unavailable, "configuration_unavailable")
	}
	config, accounts, executor, err := c.prepare(ctx, state)
	if err != nil || ctx.Err() != nil {
		return operation.Reject(operation.Invalid, "invalid_configuration")
	}
	if err = c.save(state); err != nil {
		if errors.Is(err, ErrConfigSnapshotCapacity) {
			return operation.Reject(operation.Limited, "configuration_snapshot_capacity")
		}
		return operation.Reject(operation.Unavailable, "configuration_unavailable")
	}
	c.state = cloneConfigurationState(state)
	if err = c.engine.Configure(config, accounts, executor); err != nil {
		c.configUnavailable = true
		c.engine.DenyConfiguration()
		return operation.Reject(operation.Unavailable, "configuration_unavailable")
	}
	c.wakeCleanup()
	return operation.Accept(map[string]bool{"ok": true})
}

func (c *Configuration) beginCredentialTransition(ctx context.Context, operationID, fingerprint string, slot, key string, content ConfigContent) operation.Result {
	if !trust.ValidID(operationID) || !validDigest(fingerprint) {
		return operation.Reject(operation.Invalid, "validation")
	}
	if result, found := c.operationStatus(ctx, operationID, fingerprint); found {
		return result
	}
	if c.configUnavailable {
		return operation.Reject(operation.Unavailable, "configuration_unavailable")
	}
	if len(c.state.Receipts) >= maxConfigurationReceipts {
		return operation.Reject(operation.Limited, "configuration_receipt_capacity")
	}
	if len(c.state.OwnedSlots) >= maxOwnedCredentialSlots {
		return operation.Reject(operation.Limited, "credential_slot_capacity")
	}
	if c.state.Revision >= ^uint64(0)-1 {
		return operation.Reject(operation.Limited, "configuration_revision_capacity")
	}
	retireCount := 0
	for _, oldSlot := range c.state.OwnedSlots {
		if contentHasSlot(configurationContent(c.state), oldSlot) && !contentHasSlot(content, oldSlot) {
			retireCount++
		}
	}
	if len(c.state.Cleanup)+retireCount > maxCredentialCleanup {
		return operation.Reject(operation.Limited, "credential_cleanup_capacity")
	}
	// Confirm the freshly issued reference is absent before persisting intent.
	// The create-only store then prevents replacement after this point.
	value, err := c.credentials.ReadProviderCredential(ctx, slot)
	if err != nil || value != "" {
		return operation.Reject(operation.Unavailable, "credential_store_unavailable")
	}
	if c.state.Revision == ^uint64(0) {
		return operation.Reject(operation.Limited, "configuration_revision_capacity")
	}
	pendingState := cloneConfigurationState(c.state)
	pendingState.Revision++
	pendingState.Pending = &CredentialTransition{OperationID: operationID, Fingerprint: fingerprint, Slot: slot, Digest: trust.Digest(key), Candidate: cloneConfigContent(content)}
	if !configurationStateFitsBound(pendingState) {
		return operation.Reject(operation.Limited, "configuration_snapshot_capacity")
	}
	candidateState := withConfigurationContent(cloneConfigurationState(c.state), cloneConfigContent(content))
	if _, _, _, err := c.prepare(ctx, candidateState); err != nil {
		return operation.Reject(operation.Invalid, "invalid_configuration")
	}
	if _, err := c.committedPendingState(pendingState, c.engine.Generation()); err != nil {
		if errors.Is(err, ErrConfigSnapshotCapacity) {
			return operation.Reject(operation.Limited, "configuration_snapshot_capacity")
		}
		return operation.Reject(operation.Limited, "credential_cleanup_capacity")
	}
	if err := c.save(pendingState); err != nil {
		if errors.Is(err, ErrConfigSnapshotCapacity) {
			return operation.Reject(operation.Limited, "configuration_snapshot_capacity")
		}
		return operation.Reject(operation.Unavailable, "configuration_unavailable")
	}
	c.state = pendingState
	writeErr := c.credentials.CreateProviderCredential(ctx, slot, key)
	observed, readErr := c.credentials.ReadProviderCredential(ctx, slot)
	if readErr != nil {
		c.configUnavailable = true
		c.engine.DenyConfiguration()
		return operation.Reject(operation.Unavailable, "credential_store_unavailable")
	}
	if observed == "" {
		if writeErr != nil {
			if err := c.abortPending(operationID, fingerprint); err != nil {
				return operation.Reject(operation.Unavailable, "configuration_unavailable")
			}
			return operation.Reject(operation.Unavailable, "credential_store_unavailable")
		}
		// A successful create followed by an absent readback is ambiguous.
		c.configUnavailable = true
		c.engine.DenyConfiguration()
		return operation.Reject(operation.Unavailable, "credential_store_unavailable")
	}
	if trust.Digest(observed) != pendingState.Pending.Digest {
		c.configUnavailable = true
		c.engine.DenyConfiguration()
		return operation.Reject(operation.Unavailable, "credential_store_unavailable")
	}
	if err := c.finishPending(ctx, false); err != nil {
		return operation.Reject(operation.Unavailable, "configuration_unavailable")
	}
	return operation.Accept(map[string]bool{"ok": true})
}

func (c *Configuration) abortPending(operationID, fingerprint string) error {
	if len(c.state.Receipts) >= maxConfigurationReceipts || c.state.Revision == ^uint64(0) {
		return errors.New("configuration receipt capacity exhausted")
	}
	next := cloneConfigurationState(c.state)
	next.Pending = nil
	next.Revision++
	next.Receipts = append(next.Receipts, ConfigurationReceipt{OperationID: operationID, Fingerprint: fingerprint, Category: string(operation.Unavailable), Code: "credential_store_unavailable"})
	if err := c.save(next); err != nil {
		return err
	}
	c.state = next
	return nil
}

// resolvePending interprets only exact-slot readback. Empty is authoritative
// absence; any read error or digest mismatch leaves the owner denied/pending.
func (c *Configuration) resolvePending(ctx context.Context, startup bool) error {
	if c.state.Pending == nil {
		return nil
	}
	pending := *c.state.Pending
	value, err := c.credentials.ReadProviderCredential(ctx, pending.Slot)
	if err != nil {
		c.configUnavailable = true
		c.engine.DenyConfiguration()
		return err
	}
	if value == "" {
		return c.abortPending(pending.OperationID, pending.Fingerprint)
	}
	if trust.Digest(value) != pending.Digest {
		c.configUnavailable = true
		c.engine.DenyConfiguration()
		return errors.New("pending credential readback is ambiguous")
	}
	return c.finishPending(ctx, startup)
}

func (c *Configuration) finishPending(ctx context.Context, startup bool) error {
	if c.state.Pending == nil {
		return nil
	}
	next, err := c.committedPendingState(c.state, c.engine.Generation())
	if err != nil {
		return err
	}
	config, accounts, executor, err := c.prepare(ctx, next)
	if err != nil {
		return err
	}
	if err = c.save(next); err != nil {
		return err
	}
	c.state = cloneConfigurationState(next)
	if !startup {
		if err = c.engine.Configure(config, accounts, executor); err != nil {
			c.configUnavailable = true
			c.engine.DenyConfiguration()
			return err
		}
	}
	c.wakeCleanup()
	return nil
}

func (c *Configuration) committedPendingState(state ConfigState, generation uint64) (ConfigState, error) {
	pending := state.Pending
	if pending == nil {
		return cloneConfigurationState(state), nil
	}
	if len(state.Receipts) >= maxConfigurationReceipts || len(state.OwnedSlots) >= maxOwnedCredentialSlots || state.Revision == ^uint64(0) {
		return ConfigState{}, errors.New("configuration lifecycle capacity exhausted")
	}
	next := cloneConfigurationState(state)
	old := state
	next = withConfigurationContent(next, cloneConfigContent(pending.Candidate))
	next.Pending = nil
	next.Revision++
	next.OwnedSlots = append(next.OwnedSlots, pending.Slot)
	next.Receipts = append(next.Receipts, ConfigurationReceipt{OperationID: pending.OperationID, Fingerprint: pending.Fingerprint, Category: string(operation.Ready), Code: "ok"})
	if err := c.planRetiredSlots(&next, old, generation); err != nil {
		return ConfigState{}, err
	}
	if !configurationStateFitsBound(next) {
		return ConfigState{}, ErrConfigSnapshotCapacity
	}
	return next, nil
}

func contentHasSlot(content ConfigContent, slot string) bool {
	for _, target := range content.Targets {
		if target.APIKeyEnv == slot {
			return true
		}
	}
	for _, profile := range content.Providers {
		if profile.APIKeyEnv == slot {
			return true
		}
	}
	return false
}

func (c *Configuration) planRetiredSlots(next *ConfigState, old ConfigState, generation uint64) error {
	for _, slot := range old.OwnedSlots {
		if contentHasSlot(configurationContent(old), slot) && !contentHasSlot(configurationContent(*next), slot) {
			found := false
			for _, item := range next.Cleanup {
				if item.Slot == slot {
					found = true
					break
				}
			}
			if !found {
				if len(next.Cleanup) >= maxCredentialCleanup {
					return errors.New("credential cleanup capacity exhausted")
				}
				next.Cleanup = append(next.Cleanup, CredentialCleanup{Slot: slot, RetiredGeneration: generation})
			}
		}
	}
	return nil
}

func (c *Configuration) RecoverOperation(ctx context.Context, operator trust.OperatorPrincipal, operationID string) operation.Result {
	return c.withCurrentOperator(operator, func() operation.Result {
		if !trust.ValidID(operationID) {
			return operation.Reject(operation.Invalid, "validation")
		}
		c.mu.Lock()
		defer c.mu.Unlock()
		read := c.repository.LoadConfig()
		if read.Disposition != ConfigReadPresent && read.Disposition != ConfigReadAbsent {
			return c.denyUnavailable()
		}
		if read.Disposition == ConfigReadAbsent && (c.state.Revision != 0 || len(c.state.Targets) != 0 || len(c.state.Routes) != 0 || len(c.state.Providers) != 0 || len(c.state.OwnedSlots) != 0 || len(c.state.Cleanup) != 0 || len(c.state.Receipts) != 0 || c.state.Pending != nil) {
			c.configUnavailable = true
			c.engine.DenyConfiguration()
			return operation.Reject(operation.Unavailable, "configuration_unavailable")
		}
		state := read.State
		if read.Disposition == ConfigReadAbsent {
			state = emptyConfigurationState()
		}
		if validateConfigurationState(state, c.factory) != nil {
			return c.denyUnavailable()
		}
		if !configurationStateFitsBound(state) {
			return c.denyUnavailable()
		}
		if state.Pending != nil && state.Pending.OperationID != operationID {
			return operation.Reject(operation.Conflict, "configuration_transition_pending")
		}
		needsAdoption := c.configUnavailable || !reflect.DeepEqual(c.state, state) || state.Pending != nil
		c.state = cloneConfigurationState(state)
		if c.state.Pending != nil {
			if err := c.resolvePending(ctx, true); err != nil {
				return operation.Reject(operation.Unavailable, "configuration_unavailable")
			}
		}
		if needsAdoption {
			config, accounts, executor, err := c.prepare(ctx, c.state)
			if err != nil {
				return operation.Reject(operation.Unavailable, "configuration_unavailable")
			}
			if err = c.engine.RecoverConfiguration(config, accounts, executor); err != nil {
				c.configUnavailable = true
				return operation.Reject(operation.Unavailable, "configuration_unavailable")
			}
			c.configUnavailable = false
		}
		if err := c.cleanupDurable(ctx, false); err != nil {
			return operation.Reject(operation.Unavailable, "configuration_unavailable")
		}
		for _, receipt := range c.state.Receipts {
			if receipt.OperationID == operationID {
				if receipt.Category == string(operation.Ready) {
					return operation.Accept(map[string]any{"ok": true, "recovered": true})
				}
				return operation.Accept(map[string]any{"ok": true, "recovered": true, "category": receipt.Category, "code": receipt.Code})
			}
		}
		return operation.Accept(map[string]any{"ok": true, "recovered": true, "status": "no_record"})
	})
}

func (c *Configuration) denyUnavailable() operation.Result {
	c.configUnavailable = true
	c.engine.DenyConfiguration()
	return operation.Reject(operation.Unavailable, "configuration_unavailable")
}

func (c *Configuration) cleanupDurable(ctx context.Context, restart bool) error {
	for _, item := range append([]CredentialCleanup(nil), c.state.Cleanup...) {
		if contentHasSlot(configurationContent(c.state), item.Slot) {
			return errors.New("cleanup slot still referenced")
		}
		if !restart && !c.engine.GenerationDrained(item.RetiredGeneration) {
			continue
		}
		deleteErr := c.credentials.DeleteProviderCredential(ctx, item.Slot)
		value, readErr := c.credentials.ReadProviderCredential(ctx, item.Slot)
		if readErr != nil {
			return readErr
		}
		if value != "" {
			if deleteErr != nil {
				return deleteErr
			}
			return errors.New("credential deletion not confirmed")
		}
		if c.state.Revision == ^uint64(0) {
			return errors.New("configuration revision capacity exhausted")
		}
		next := cloneConfigurationState(c.state)
		next.Revision++
		for i := range next.Cleanup {
			if next.Cleanup[i].Slot == item.Slot {
				next.Cleanup = append(next.Cleanup[:i], next.Cleanup[i+1:]...)
				break
			}
		}
		for i, slot := range next.OwnedSlots {
			if slot == item.Slot {
				next.OwnedSlots = append(next.OwnedSlots[:i], next.OwnedSlots[i+1:]...)
				break
			}
		}
		if err := c.save(next); err != nil {
			return err
		}
		c.state = next
	}
	return nil
}

func (c *Configuration) wakeCleanup() {
	if c.cleanupWake != nil {
		select {
		case c.cleanupWake <- struct{}{}:
		default:
		}
	}
}

func (c *Configuration) cleanupLoop() {
	defer close(c.cleanupDone)
	ticker := time.NewTicker(time.Second)
	defer ticker.Stop()
	for {
		select {
		case <-c.cleanupStop:
			return
		case <-ticker.C:
		case <-c.cleanupWake:
		}
		c.mu.Lock()
		if !c.configUnavailable {
			_ = c.cleanupDurable(context.Background(), false)
		}
		c.mu.Unlock()
	}
}

func (c *Configuration) Close() {
	if c == nil || c.cleanupStop == nil {
		return
	}
	c.cleanupCloseOnce.Do(func() { close(c.cleanupStop) })
	<-c.cleanupDone
}
