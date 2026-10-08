//go:build windows

package terminal

import (
	"os"
	"time"
)

// drain on Windows is a no-op because POSIX poll is unavailable.
func drain(tty *os.File) {
}

// pollRead on Windows returns true to allow standard console read.
func pollRead(tty *os.File, wait time.Duration) bool {
	return true
}
