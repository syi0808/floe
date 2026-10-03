package credentials

import (
	"context"
	"errors"
	"strings"
	"time"
)

var ErrUnavailable = errors.New("credential store unavailable")
var ErrLocked = errors.New("credential store locked")
var ErrBusy = errors.New("credential store busy")

// A process-wide native lane admits one OS call and at most eight observers
// waiting for it. The worker owns the lane until write plus exact readback has
// settled, even when its observer leaves. Reads cannot overtake an uncertain
// mutation and no second OS worker is created while the first is stalled.
var nativeLane = make(chan struct{}, 1)
var nativeObservers = make(chan struct{}, 8)

type Keychain struct{}
type nativeResult struct {
	value string
	err   error
}

func (Keychain) Get(ctx context.Context, name string) (string, error) {
	return observeNative(ctx, name, "", 0)
}
func (Keychain) Put(ctx context.Context, name, value string) error {
	_, err := observeNative(ctx, name, value, 1)
	return err
}
func (Keychain) Delete(ctx context.Context, name string) error {
	_, err := observeNative(ctx, name, "", 2)
	return err
}

func observeNative(parent context.Context, name, value string, operation int) (string, error) {
	if parent == nil || operation < 0 || operation > 2 || (operation == 1 && value == "") || name == "" || len(name) > 256 || len(value) > 131072 || strings.ContainsRune(name+value, 0) {
		return "", ErrUnavailable
	}
	ctx, cancel := context.WithTimeout(parent, 3*time.Second)
	defer cancel()
	if err := ctx.Err(); err != nil {
		return "", err
	}
	select {
	case nativeObservers <- struct{}{}:
	default:
		return "", ErrBusy
	}
	defer func() { <-nativeObservers }()
	select {
	case nativeLane <- struct{}{}:
	case <-ctx.Done():
		return "", ctx.Err()
	}
	if err := ctx.Err(); err != nil {
		<-nativeLane
		return "", err
	}
	result := make(chan nativeResult, 1)
	go func() {
		defer func() { <-nativeLane }()
		if err := ctx.Err(); err != nil {
			result <- nativeResult{err: err}
			return
		}
		actual, err := nativeKeychain(name, value, operation)
		if err == nil && operation != 0 {
			actual, err = nativeKeychain(name, "", 0)
			if err == nil && (operation == 1 && actual != value || operation == 2 && actual != "") {
				err = ErrUnavailable
			}
		}
		result <- nativeResult{actual, err}
	}()
	select {
	case outcome := <-result:
		if err := ctx.Err(); err != nil {
			return "", err
		}
		return outcome.value, outcome.err
	case <-ctx.Done():
		return "", ctx.Err()
	}
}
