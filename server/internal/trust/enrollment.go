package trust

import "time"

// EnrollmentLifetime bounds the human code-comparison and administrator approval
// ceremony. It is not the short-lived source admission/release proof lifetime.
// The original signed deadline is immutable across confirmation and replay.
const EnrollmentLifetime = 5 * time.Minute

func validEnrollmentWindow(issued, expires, now int64) bool {
	return issued > 0 && issued <= now && expires > now &&
		expires > issued && expires-issued <= EnrollmentLifetime.Milliseconds()
}
