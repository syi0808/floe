package views

import (
	"context"
	"testing"
	"time"
)

func TestClaimFinishCannotMutateReplacementEntry(t *testing.T) {
	const id = "same-challenge-id"
	oldToken, newToken := &claimToken{}, &claimToken{}
	expires := time.Now().Add(-time.Hour)

	service := &Service{
		clock: wallClock{},
		admissions: map[string]admissionState{
			id: {claiming: true, claimToken: newToken, expires: expires},
		},
		releases: map[string]releaseState{
			id: {admissionState: admissionState{claiming: true, claimToken: newToken, expires: expires}},
		},
	}
	service.finishAdmissionClaim(id, oldToken, false)
	service.finishReleaseClaim(id, oldToken, false)
	if state := service.admissions[id]; !state.claiming || state.claimToken != newToken {
		t.Fatal("stale admission completion changed the replacement claim")
	}
	if state := service.releases[id]; !state.claiming || state.claimToken != newToken {
		t.Fatal("stale release completion changed the replacement claim")
	}

	mirror := &CalendarMirrorService{
		clock: wallClock{},
		pending: map[string]mirrorPending{
			id: {claiming: true, claimToken: newToken, expires: expires},
		},
		releases: map[string]mirrorRelease{
			id: {claiming: true, claimToken: newToken, expires: expires},
		},
	}
	mirror.finishMirrorAdmissionClaim(id, oldToken, false)
	mirror.finishMirrorReleaseClaim(id, oldToken, false)
	if state := mirror.pending[id]; !state.claiming || state.claimToken != newToken {
		t.Fatal("stale Mirror admission completion changed the replacement claim")
	}
	if state := mirror.releases[id]; !state.claiming || state.claimToken != newToken {
		t.Fatal("stale Mirror release completion changed the replacement claim")
	}
}

func TestServicesCloseCancelsAndDrainsOperations(t *testing.T) {
	t.Run("views", func(t *testing.T) {
		lifetime, cancel := context.WithCancel(context.Background())
		sweepDone := make(chan struct{})
		close(sweepDone)
		service := &Service{lifetime: lifetime, sweepCancel: cancel, sweepDone: sweepDone, admissions: map[string]admissionState{}, releases: map[string]releaseState{}}
		operation, done, ok := service.beginOperation(context.Background())
		if !ok {
			t.Fatal("open Views service refused an operation")
		}
		closeDone := make(chan struct{})
		go func() { service.Close(); close(closeDone) }()
		assertCloseCancelsAndWaits(t, operation, done, closeDone, func() {
			if _, _, ok := service.beginOperation(context.Background()); ok {
				t.Fatal("closed Views service admitted new work")
			}
		})
	})
	t.Run("calendar mirror", func(t *testing.T) {
		lifetime, cancel := context.WithCancel(context.Background())
		sweepDone := make(chan struct{})
		close(sweepDone)
		service := &CalendarMirrorService{lifetime: lifetime, sweepCancel: cancel, sweepDone: sweepDone, reads: map[string]*mirrorRead{}, pending: map[string]mirrorPending{}, releases: map[string]mirrorRelease{}}
		operation, done, ok := service.beginOperation(context.Background())
		if !ok {
			t.Fatal("open Calendar Mirror service refused an operation")
		}
		closeDone := make(chan struct{})
		go func() { service.Close(); close(closeDone) }()
		assertCloseCancelsAndWaits(t, operation, done, closeDone, func() {
			if _, _, ok := service.beginOperation(context.Background()); ok {
				t.Fatal("closed Calendar Mirror service admitted new work")
			}
		})
	})
}

func assertCloseCancelsAndWaits(t *testing.T, operation context.Context, done func(), closeDone <-chan struct{}, closed func()) {
	t.Helper()
	select {
	case <-operation.Done():
	case <-time.After(2 * time.Second):
		t.Fatal("Close did not cancel the active operation context")
	}
	select {
	case <-closeDone:
		t.Fatal("Close returned before the active operation drained")
	default:
	}
	closed()
	done()
	select {
	case <-closeDone:
	case <-time.After(2 * time.Second):
		t.Fatal("Close did not finish after the active operation drained")
	}
}
