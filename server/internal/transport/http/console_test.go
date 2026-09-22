package httptransport

import (
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestManagementPreservesWrongMethodStatus(test *testing.T) {
	for _, candidate := range []struct {
		path   string
		status int
	}{
		{"/manage/api/pair/approve", http.StatusMethodNotAllowed},
		{"/manage/api/pair/reject", http.StatusMethodNotAllowed},
		{"/manage/api/route", http.StatusNotFound},
		{"/manage/api/provider", http.StatusNotFound},
		{"/manage/api/target", http.StatusNotFound},
		{"/manage/api/test", http.StatusNotFound},
		{"/manage/api/client/delete", http.StatusNotFound},
		{"/manage/api/target/delete", http.StatusNotFound},
		{"/manage/api/codex/status", http.StatusNotFound},
		{"/manage/api/logout", http.StatusNotFound},
		{"/manage/api/unknown", http.StatusNotFound},
	} {
		test.Run(candidate.path, func(test *testing.T) {
			handler := &Handler{}
			response := httptest.NewRecorder()
			request := httptest.NewRequest(http.MethodGet, candidate.path, nil)
			handler.manage(response, request, "", sessionRecord{})
			if response.Code != candidate.status {
				test.Fatalf("status = %d, want %d", response.Code, candidate.status)
			}
		})
	}
}
