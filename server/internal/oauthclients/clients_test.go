package oauthclients

import "testing"

func TestBundledClientIDs(t *testing.T) {
	for _, name := range []string{
		"FLOE_GOOGLE_OAUTH_CLIENT_ID",
		"FLOE_GITHUB_OAUTH_CLIENT_ID",
		"FLOE_SLACK_OAUTH_CLIENT_ID",
	} {
		if ClientID(name) == "" {
			t.Fatalf("missing bundled client ID %s", name)
		}
	}
	if ClientID("FLOE_MICROSOFT_OAUTH_CLIENT_ID") != "" {
		t.Fatal("Microsoft OAuth must remain opt-in")
	}
}

func TestParseIgnoresSecretsAndInvalidLines(t *testing.T) {
	values := parse("# public values\n FLOE_GOOGLE_OAUTH_CLIENT_ID = google \nFLOE_GOOGLE_OAUTH_CLIENT_SECRET=secret\ninvalid\n")
	if values["FLOE_GOOGLE_OAUTH_CLIENT_ID"] != "google" || len(values) != 1 {
		t.Fatalf("unexpected values: %#v", values)
	}
}
