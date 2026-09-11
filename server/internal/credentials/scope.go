package credentials

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"regexp"
	"strings"
)

var connectionIDPattern = regexp.MustCompile(`^[A-Za-z0-9._:-]{1,128}$`)
var personIDPattern = regexp.MustCompile(`^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-5][0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$`)

func ConnectionName(namespace, connectionID, personID string) (string, error) {
	if !connectionIDPattern.MatchString(namespace) || !connectionIDPattern.MatchString(connectionID) || !personIDPattern.MatchString(personID) {
		return "", errors.New("invalid connection credential scope")
	}
	hash := sha256.Sum256([]byte(strings.ToLower(personID) + "\x00" + connectionID))
	return namespace + ":" + hex.EncodeToString(hash[:]), nil
}
