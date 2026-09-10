package googledrive

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/url"
	"regexp"
	"strings"
	"time"
	"unicode/utf8"

	"floe/server/internal/connectors/common"
)

const (
	defaultBaseURL = "https://www.googleapis.com"
	maxFiles       = 8
	maxListBytes   = 128 * 1024
	maxFileBytes   = 16 * 1024
)

var (
	idPattern = regexp.MustCompile(`^[A-Za-z0-9_-]{10,128}$`)

	ErrInvalidInput      = errors.New("invalid Google Drive input")
	ErrCredentialExpired = errors.New("Google Drive credential expired")
	ErrPermissionDenied  = errors.New("Google Drive permission denied")
	ErrRateLimited       = errors.New("Google Drive rate limited")
	ErrUnavailable       = errors.New("Google Drive unavailable")
	ErrInvalidResponse   = errors.New("invalid Google Drive response")
)

type TokenSource interface {
	Token(context.Context) (string, error)
}

type Client struct {
	tokens  TokenSource
	baseURL string
	http    *http.Client
}

type fileList struct {
	NextPageToken string `json:"nextPageToken"`
	Files         []struct {
		ID           string `json:"id"`
		Name         string `json:"name"`
		MimeType     string `json:"mimeType"`
		ModifiedTime string `json:"modifiedTime"`
	} `json:"files"`
}

func New(tokens TokenSource) (*Client, error) {
	return NewWithBaseURL(tokens, defaultBaseURL)
}

func NewWithBaseURL(tokens TokenSource, baseURL string) (*Client, error) {
	if tokens == nil {
		return nil, ErrInvalidInput
	}
	endpoint, err := url.Parse(baseURL)
	if err != nil || endpoint.User != nil || endpoint.RawQuery != "" || endpoint.Fragment != "" || strings.TrimSuffix(endpoint.Path, "/") != "" {
		return nil, ErrInvalidInput
	}
	loopback := endpoint.Scheme == "http" && (endpoint.Hostname() == "127.0.0.1" || endpoint.Hostname() == "localhost")
	if endpoint.Scheme != "https" && !loopback {
		return nil, ErrInvalidInput
	}
	return &Client{
		tokens:  tokens,
		baseURL: strings.TrimSuffix(endpoint.String(), "/"),
		http: &http.Client{
			Timeout:   15 * time.Second,
			Transport: &http.Transport{Proxy: nil},
			CheckRedirect: func(*http.Request, []*http.Request) error {
				return http.ErrUseLastResponse
			},
		},
	}, nil
}

func (client *Client) WorkContext(ctx context.Context, folderID string, now time.Time) (common.WorkContextView, error) {
	if !idPattern.MatchString(folderID) {
		return common.WorkContextView{}, ErrInvalidInput
	}
	query := url.Values{
		"fields":   {"nextPageToken,files(id,name,mimeType,modifiedTime)"},
		"orderBy":  {"modifiedTime desc"},
		"pageSize": {"8"},
		"q":        {"'" + folderID + "' in parents and trashed = false"},
		"spaces":   {"drive"},
	}
	var files fileList
	if err := client.getJSON(ctx, "/drive/v3/files?"+query.Encode(), &files); err != nil {
		return common.WorkContextView{}, err
	}
	if len(files.Files) > maxFiles {
		return common.WorkContextView{}, ErrInvalidResponse
	}
	view := common.WorkContextView{
		SchemaVersion:    1,
		ViewID:           "work.context",
		SourceHandle:     handle("drive", folderID+":"+now.UTC().Format("2006-01-02T15:04")),
		ObservedAtUnixMS: now.UnixMilli(),
		ExpiresAtUnixMS:  now.Add(5 * time.Minute).UnixMilli(),
		CoverageComplete: strings.TrimSpace(files.NextPageToken) == "",
		ScopeHandle:      handle("folder", folderID),
		Items:            []common.WorkItem{},
	}
	seen := map[string]bool{}
	for _, file := range files.Files {
		if !supportedMIME(file.MimeType) {
			continue
		}
		modified, err := time.Parse(time.RFC3339Nano, file.ModifiedTime)
		if err != nil || !idPattern.MatchString(file.ID) || seen[file.ID] || strings.TrimSpace(file.Name) == "" || len(file.Name) > 512 || modified.After(now.Add(time.Minute)) {
			return common.WorkContextView{}, ErrInvalidResponse
		}
		seen[file.ID] = true
		path := "/drive/v3/files/" + url.PathEscape(file.ID) + "?alt=media"
		if file.MimeType == "application/vnd.google-apps.document" {
			path = "/drive/v3/files/" + url.PathEscape(file.ID) + "/export?mimeType=" + url.QueryEscape("text/plain")
		}
		content, err := client.getBytes(ctx, path, maxFileBytes)
		if err != nil || !utf8.Valid(content) {
			return common.WorkContextView{}, ErrInvalidResponse
		}
		excerpt := truncate(strings.TrimSpace(string(content)), 2048)
		var excerptPointer *string
		if excerpt != "" {
			excerptPointer = &excerpt
		}
		status := "selected_file"
		view.Items = append(view.Items, common.WorkItem{
			EvidenceHandle:   handle("drive", folderID+":"+file.ID+":"+file.ModifiedTime),
			Kind:             "selected_file",
			Title:            strings.TrimSpace(file.Name),
			Excerpt:          excerptPointer,
			Status:           &status,
			ObservedAtUnixMS: modified.UnixMilli(),
		})
	}
	encoded, err := json.Marshal(view)
	if err != nil || len(encoded) > 65_536 {
		return common.WorkContextView{}, ErrInvalidResponse
	}
	return view, nil
}

func (client *Client) getJSON(ctx context.Context, path string, output any) error {
	data, err := client.getBytes(ctx, path, maxListBytes)
	if err != nil {
		return err
	}
	if json.Unmarshal(data, output) != nil {
		return ErrInvalidResponse
	}
	return nil
}

func (client *Client) getBytes(ctx context.Context, path string, maximum int64) ([]byte, error) {
	token, err := client.tokens.Token(ctx)
	if err != nil || len(token) < 8 || len(token) > 16_384 || strings.ContainsAny(token, "\r\n") {
		return nil, ErrCredentialExpired
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, client.baseURL+path, nil)
	if err != nil {
		return nil, ErrInvalidInput
	}
	request.Header.Set("Authorization", "Bearer "+token)
	response, err := client.http.Do(request)
	if err != nil {
		return nil, ErrUnavailable
	}
	defer response.Body.Close()
	switch response.StatusCode {
	case http.StatusOK:
	case http.StatusUnauthorized:
		return nil, ErrCredentialExpired
	case http.StatusForbidden:
		return nil, ErrPermissionDenied
	case http.StatusTooManyRequests:
		return nil, ErrRateLimited
	default:
		return nil, ErrUnavailable
	}
	data, err := io.ReadAll(io.LimitReader(response.Body, maximum+1))
	if err != nil || int64(len(data)) > maximum {
		return nil, ErrInvalidResponse
	}
	return data, nil
}

func supportedMIME(value string) bool {
	switch value {
	case "text/plain", "text/markdown", "text/csv", "application/json", "application/xml", "application/vnd.google-apps.document":
		return true
	default:
		return false
	}
}

func handle(namespace, value string) string {
	digest := sha256.Sum256([]byte(namespace + "\x00" + value))
	return namespace + ":" + hex.EncodeToString(digest[:])
}

func truncate(value string, maximum int) string {
	if len(value) <= maximum {
		return value
	}
	for maximum > 0 && maximum < len(value) && value[maximum]&0xc0 == 0x80 {
		maximum--
	}
	return strings.TrimSpace(value[:maximum])
}
