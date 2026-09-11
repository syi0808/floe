package credentials

import "testing"

func TestConnectionNameIsolatesPersonAndConnection(test *testing.T) {
	first, err := ConnectionName("FLOE_GOOGLE_CALENDAR_OAUTH", "calendar.google.primary", "00000000-0000-4000-8000-000000000001")
	if err != nil {
		test.Fatal(err)
	}
	repeated, _ := ConnectionName("FLOE_GOOGLE_CALENDAR_OAUTH", "calendar.google.primary", "00000000-0000-4000-8000-000000000001")
	otherPerson, _ := ConnectionName("FLOE_GOOGLE_CALENDAR_OAUTH", "calendar.google.primary", "00000000-0000-4000-8000-000000000002")
	otherConnection, _ := ConnectionName("FLOE_GOOGLE_CALENDAR_OAUTH", "calendar.google.secondary", "00000000-0000-4000-8000-000000000001")
	if first != repeated || first == otherPerson || first == otherConnection {
		test.Fatalf("credential scope collision: %q %q %q", first, otherPerson, otherConnection)
	}
}

func TestConnectionNameRejectsUnboundedScope(test *testing.T) {
	for _, sample := range [][3]string{
		{"bad namespace", "calendar.google", "00000000-0000-4000-8000-000000000001"},
		{"oauth", "../calendar", "00000000-0000-4000-8000-000000000001"},
		{"oauth", "calendar.google", "person"},
	} {
		if _, err := ConnectionName(sample[0], sample[1], sample[2]); err == nil {
			test.Fatalf("invalid scope accepted: %#v", sample)
		}
	}
}
