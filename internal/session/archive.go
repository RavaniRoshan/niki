package session

import (
	"compress/gzip"
	"io"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// ArchiveSummary tracks the count and byte reduction of an archive sweep.
type ArchiveSummary struct {
	ScannedCount  int   `json:"scanned_count"`
	ArchivedCount int   `json:"archived_count"`
	BytesSaved    int64 `json:"bytes_saved"`
}

// ArchiveColdSessions scans sessionsDir for inactive session JSONL logs older than maxAge and compresses them into .jsonl.gz.
func ArchiveColdSessions(sessionsDir string, maxAge time.Duration) (ArchiveSummary, error) {
	var summary ArchiveSummary

	entries, err := os.ReadDir(sessionsDir)
	if err != nil {
		return summary, err
	}

	now := time.Now()
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".jsonl") {
			continue
		}
		summary.ScannedCount++

		fullPath := filepath.Join(sessionsDir, entry.Name())
		info, err := entry.Info()
		if err != nil {
			continue
		}

		if now.Sub(info.ModTime()) > maxAge {
			origSize := info.Size()
			gzPath := fullPath + ".gz"

			if err := compressFile(fullPath, gzPath); err != nil {
				continue
			}

			gzInfo, err := os.Stat(gzPath)
			if err == nil {
				summary.BytesSaved += (origSize - gzInfo.Size())
			}

			_ = os.Remove(fullPath)
			summary.ArchivedCount++
		}
	}

	return summary, nil
}

func compressFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()

	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	defer out.Close()

	gw := gzip.NewWriter(out)
	defer gw.Close()

	_, err = io.Copy(gw, in)
	return err
}
