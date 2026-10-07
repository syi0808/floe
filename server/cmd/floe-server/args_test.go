package main

import "testing"

func TestParseArgsDefaultsAndTokenRetrieval(t *testing.T) {
	defaults, err := parseArgs(nil)
	if err != nil || defaults.printAdminToken || defaults.developmentQANoAuth {
		t.Fatalf("default arguments = %#v, %v", defaults, err)
	}

	token, err := parseArgs([]string{"--print-admin-token"})
	if err != nil || !token.printAdminToken || token.developmentQANoAuth {
		t.Fatalf("token retrieval arguments = %#v, %v", token, err)
	}
}

func TestDevelopmentQANoAuthFlagIsBuildGated(t *testing.T) {
	got, err := parseArgs([]string{"--dev-qa-no-auth"})
	if developmentQAFlagAvailable() {
		if err != nil || !got.developmentQANoAuth {
			t.Fatalf("development QA flag = %#v, %v", got, err)
		}
	} else if err == nil {
		t.Fatalf("production build accepted --dev-qa-no-auth: %#v", got)
	}
	if _, err := parseArgs([]string{"--print-admin-token", "--dev-qa-no-auth"}); err == nil {
		t.Fatal("combined token retrieval and QA arguments were accepted")
	}
}
