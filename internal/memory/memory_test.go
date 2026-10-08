package memory

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestMemoryIndexCapsAndRetrieval(t *testing.T) {
	tmpDir := t.TempDir()
	store, err := NewStore(tmpDir)
	if err != nil {
		t.Fatalf("failed creating store: %v", err)
	}

	// 1. Add 250 facts to test the <= 200 lines and <= 25 KB limits
	for i := 1; i <= 250; i++ {
		err := store.AddFact("database", fmt.Sprintf("Fact %d: Use PostgreSQL for service %d with pooling", i, i))
		if err != nil {
			t.Fatalf("add fact error: %v", err)
		}
	}

	indexData, err := os.ReadFile(store.indexPath)
	if err != nil {
		t.Fatalf("failed reading MEMORY.md: %v", err)
	}

	lines := strings.Split(string(indexData), "\n")
	if len(lines) > MaxIndexLines {
		t.Fatalf("MEMORY.md lines (%d) exceed MaxIndexLines (%d)", len(lines), MaxIndexLines)
	}
	if len(indexData) > MaxIndexBytes {
		t.Fatalf("MEMORY.md bytes (%d) exceed MaxIndexBytes (%d)", len(indexData), MaxIndexBytes)
	}

	// 2. Add multiple topics for retrieval test
	_ = store.AddFact("auth", "Use JWT tokens with RS256 signing")
	_ = store.AddFact("frontend", "Use Tailwind CSS and React components")
	_ = store.AddFact("caching", "Use Redis for session cache with 15m TTL")
	_ = store.AddFact("queue", "RabbitMQ is used for background jobs")
	_ = store.AddFact("storage", "S3 compatible storage for assets")
	_ = store.AddFact("search", "Meilisearch for product catalog search")

	// 3. Retrieval side-query: <= 5 topics returned
	retrieved, err := store.Retrieve("cache token database storage redis jobs", 10)
	if err != nil {
		t.Fatalf("retrieve error: %v", err)
	}
	if len(retrieved) > MaxRetrievalTopics {
		t.Fatalf("retrieved topics count (%d) exceeds limit (%d)", len(retrieved), MaxRetrievalTopics)
	}
	if len(retrieved) == 0 {
		t.Fatalf("expected at least one relevant topic retrieved")
	}

	// 4. Extraction from turn text
	turnText := "Key decision: We will use SQLite for local caching. User preference: Do not use external CSS libraries."
	extracted := store.ExtractFromTurn(turnText)
	if len(extracted) != 2 {
		t.Fatalf("expected 2 extracted facts, got %v", extracted)
	}

	// 5. Consolidation test
	if err := store.Consolidate(); err != nil {
		t.Fatalf("consolidate error: %v", err)
	}
	// Verify topic file exists and has content
	topicPath := filepath.Join(store.topicDir, "database.md")
	data, err := os.ReadFile(topicPath)
	if err != nil || len(data) == 0 {
		t.Fatalf("database topic file missing or empty after consolidate")
	}
}
