//go:build !windows

package terminal

import (
	"os"
	"time"

	"golang.org/x/sys/unix"
)

// drain reads bytes already pending on the tty without
// waiting, so a zero-length poll cannot stall startup.
func drain(tty *os.File) {
	for i := 0; i < 16; i++ {
		fds := []unix.PollFd{{Fd: int32(tty.Fd()), Events: unix.POLLIN}}
		n, err := unix.Poll(fds, 0)
		if err != nil || n == 0 {
			return
		}
		buf := make([]byte, 512)
		if _, err := tty.Read(buf); err != nil {
			return
		}
	}
}

func pollRead(tty *os.File, wait time.Duration) bool {
	fds := []unix.PollFd{{Fd: int32(tty.Fd()), Events: unix.POLLIN}}
	n, err := unix.Poll(fds, int(wait.Milliseconds()))
	return err == nil && n > 0
}
