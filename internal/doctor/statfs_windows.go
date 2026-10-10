//go:build windows

package doctor

import "errors"

// statfsFree is unsupported on native Windows (UNSUPPORTED platform, D-05);
// the disk-space check degrades to a warn with an exact fix.
func statfsFree(path string) (uint64, error) {
	return 0, errors.New("disk-space probe not implemented on native Windows (unsupported platform)")
}
