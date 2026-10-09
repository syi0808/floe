package views

import contracts "floe/server/internal/views/contracts"

type ID = contracts.ID

const (
	Calendar       ID = contracts.Calendar
	Communication  ID = contracts.Communication
	WorkContext    ID = contracts.WorkContext
	Logistics      ID = contracts.Logistics
	CalendarMirror ID = contracts.CalendarMirror
)

type SourceTarget = contracts.SourceTarget
type SourceReference = contracts.SourceReference
type SourceSnapshot = contracts.SourceSnapshot
type Bounds = contracts.Bounds
type ViewDescriptor = contracts.ViewDescriptor
type ViewSnapshot = contracts.ViewSnapshot
type CalendarQuery = contracts.CalendarQuery
type MailQuery = contracts.MailQuery
type WorkQuery = contracts.WorkQuery
type LogisticsQuery = contracts.LogisticsQuery
type CalendarMirrorQuery = contracts.CalendarMirrorQuery
type Query = contracts.Query
type ParsedQuery = contracts.ParsedQuery
type ReadRequest = contracts.ReadRequest
type Reader = contracts.Reader
type ReaderFunc = contracts.ReaderFunc
type Result = contracts.Result
type ResolvedSource = contracts.ResolvedSource
type Resolver = contracts.Resolver
type CalendarItem = contracts.CalendarItem
type CalendarView = contracts.CalendarView
type CommunicationItem = contracts.CommunicationItem
type CommunicationView = contracts.CommunicationView
type WorkItem = contracts.WorkItem
type WorkContextView = contracts.WorkContextView
type LogisticsItem = contracts.LogisticsItem
type LogisticsView = contracts.LogisticsView
type CalendarExternalRevision = contracts.CalendarExternalRevision
type CalendarSchedule = contracts.CalendarSchedule
type CalendarRecord = contracts.CalendarRecord
type CalendarMirrorResult = contracts.CalendarMirrorResult
type ReadErrorKind = contracts.ReadErrorKind
type ReadError = contracts.ReadError

const (
	InvalidQuery            = contracts.InvalidQuery
	CredentialExpired       = contracts.CredentialExpired
	PermissionDenied        = contracts.PermissionDenied
	RateLimited             = contracts.RateLimited
	Unavailable             = contracts.Unavailable
	InvalidProviderResponse = contracts.InvalidProviderResponse
)

var ErrInvalid = contracts.ErrInvalid

func ParseQuery(id ID, raw []byte) (ParsedQuery, error) { return contracts.ParseQuery(id, raw) }
func ParseMirrorQuery(raw []byte) (CalendarMirrorQuery, error) {
	return contracts.ParseMirrorQuery(raw)
}
func EncodeBounded(result Result, bounds Bounds) ([]byte, uint32, error) {
	return contracts.EncodeBounded(result, bounds)
}
func DecodeBounded(raw []byte, id ID, bounds Bounds) (Result, error) {
	return contracts.DecodeBounded(raw, id, bounds)
}
func CloneSource(snapshot SourceSnapshot) SourceSnapshot { return contracts.CloneSource(snapshot) }
func ValidateResultRequest(result Result, request ReadRequest) error {
	return contracts.ValidateResultRequest(result, request)
}
func ValidateQueryBounds(query Query, bounds Bounds) error {
	return contracts.ValidateQueryBounds(query, bounds)
}
func ValidateMirrorResult(result CalendarMirrorResult, request ReadRequest) error {
	return contracts.ValidateMirrorResult(result, request)
}
func CalendarMirrorDescriptor() ViewDescriptor { return contracts.CalendarMirrorDescriptor() }
func ObservationRevision(record CalendarRecord) CalendarExternalRevision {
	return contracts.ObservationRevision(record)
}
