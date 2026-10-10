//go:build linux || darwin

package doctor

import "golang.org/x/sys/unix"

// statfsFree returns the free bytes (available to non-root) for path's filesystem.
func statfsFree(path string) (uint64, error) {
	var st unix.Statfs_t
	if err := unix.Statfs(path, &st); err != nil {
		return 0, err
	}
	return uint64(st.Bavail) * uint64(st.Bsize), nil
}
