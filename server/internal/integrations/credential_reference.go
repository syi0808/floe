package integrations

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"regexp"
	"strings"
)

var credentialNamespacePattern = regexp.MustCompile(`^[A-Za-z0-9._:-]{1,128}$`)
var credentialConnectionPattern = regexp.MustCompile(`^[A-Za-z0-9._:-]{1,128}$`)
var credentialPersonPattern = regexp.MustCompile(`^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-5][0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$`)

// CredentialSlot is the stable connection-scoped reference persisted in a
// Record. Credential bytes remain inside the capability adapter.
func CredentialSlot(namespace, connectionID, personID string) (string, error) {
	if !credentialNamespacePattern.MatchString(namespace) || !credentialConnectionPattern.MatchString(connectionID) || !credentialPersonPattern.MatchString(personID) {
		return "", errors.New("invalid connection credential scope")
	}
	hash := sha256.Sum256([]byte(strings.ToLower(personID) + "\x00" + connectionID))
	return namespace + ":" + hex.EncodeToString(hash[:]), nil
}
