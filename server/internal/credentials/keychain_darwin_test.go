package credentials

import (
	"crypto/rand"
	"os"
	"testing"
)

func TestKeychainRoundTrip(test *testing.T) {
	if os.Getenv("FLOE_TEST_KEYCHAIN") != "1" {
		test.Skip("explicit disposable Keychain smoke test")
	}
	store := Keychain{}
	name := "floe-test-" + rand.Text()
	if err := store.Put(name, "synthetic-credential-not-a-provider-key"); err != nil {
		test.Fatal(err)
	}
	defer store.Delete(name)
	value, err := store.Get(name)
	if err != nil || value != "synthetic-credential-not-a-provider-key" {
		test.Fatal("credential round trip failed")
	}
	if err := store.Delete(name); err != nil {
		test.Fatal(err)
	}
	if _, err := store.Get(name); err == nil {
		test.Fatal("deleted credential still present")
	}
}
