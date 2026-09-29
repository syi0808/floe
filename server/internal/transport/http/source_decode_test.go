package httptransport

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"floe/server/internal/authorization"
)

func TestSourceAdmissionRejectsObsoletePolicyField(t *testing.T) {
	request := httptest.NewRequest(http.MethodPost, "/v1/views/calendar.timeline/admit", strings.NewReader(`{"policy":{"incarnation":"old","epoch":1}}`))
	response := httptest.NewRecorder()
	serveSource(response, request, authorization.Principal{}, nil)
	if response.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want %d", response.Code, http.StatusBadRequest)
	}
}
