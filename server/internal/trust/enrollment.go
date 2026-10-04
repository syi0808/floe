package trust

import "time"

// EnrollmentLifetime bounds the human code-comparison and administrator approval
// ceremony. It is not the short-lived source admission/release proof lifetime.
// The original signed deadline is immutable across confirmation and replay.
const EnrollmentLifetime = 5 * time.Minute

// ValidEnrollmentSpan also validates retained receipts after expiry, when an
// operator may abort an activation proven not to have committed.
func ValidEnrollmentSpan(issued, expires int64) bool {
	return issued > 0 && expires > issued && expires-issued <= EnrollmentLifetime.Milliseconds()
}

func validEnrollmentWindow(issued, expires, now int64) bool {
	return ValidEnrollmentSpan(issued, expires) && issued <= now && expires > now
}
