package authorization

import (
	"encoding/json"
	"testing"
)

func TestProducerIdentityPreservesUUIDVersionAndVariantValidation(test *testing.T) {
	_, encoded, err := GenerateProducerIdentity("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")
	if err != nil {
		test.Fatal(err)
	}
	var record producerIdentityRecord
	if err := json.Unmarshal(encoded, &record); err != nil {
		test.Fatal(err)
	}
	for _, candidate := range []struct {
		keyID string
		valid bool
	}{
		{"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", true},
		{"AAAAAAAA-AAAA-4AAA-Baaa-AAAAAAAAAAAA", true},
		{"aaaaaaaa-aaaa-1aaa-8aaa-aaaaaaaaaaaa", false},
		{"aaaaaaaa-aaaa-4aaa-7aaa-aaaaaaaaaaaa", false},
		{"aaaaaaaa-aaaa-4aaa-caaa-aaaaaaaaaaaa", false},
		{"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaz", false},
		{"", false},
	} {
		test.Run(candidate.keyID, func(test *testing.T) {
			record.KeyID = candidate.keyID
			data, err := json.Marshal(record)
			if err != nil {
				test.Fatal(err)
			}
			_, err = DecodeProducerIdentity(data)
			if (err == nil) != candidate.valid {
				test.Fatalf("producer key ID acceptance = %v, want %v", err == nil, candidate.valid)
			}
			if validConnectionID(candidate.keyID) != candidate.valid {
				test.Fatal("source connection ID validation changed")
			}
		})
	}
}
