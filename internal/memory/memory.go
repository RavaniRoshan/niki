package memory

import (
	"bufio"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"sync"
	"time"
)

const (
	MaxIndexLines       = 200
	MaxIndexBytes       = 25 * 1024 // 25 KB cap
	MaxRetrievalTopics  = 5         // At most 5 topics injected into context
)

type TopicFact struct {
	Timestamp time.Time `json:"timestamp"`
	Text      string    `json:"text"`
}

type Topic struct {
	Name  string      `json:"name"`
	Path  string      `json:"path"`
	Score int         `json:"score,omitempty"`
	Facts []TopicFact `json:"facts"`
}

type Store struct {
	mu        sync.RWMutex
	baseDir   string
	indexPath string
	topicDir  string
}

func NewStore(baseDir string) (*Store, error) {
	if baseDir == "" {
		home, err := os.UserHomeDir()
		if err == nil {
			baseDir = filepath.Join(home, ".niki", "memory")
		} else {
			baseDir = filepath.Join(os.TempDir(), "niki-memory")
		}
	}
	topicDir := filepath.Join(baseDir, "topics")
	if err := os.MkdirAll(topicDir, 0o755); err != nil {
		return nil, err
	}
	indexPath := filepath.Join(baseDir, "MEMORY.md")
	if _, err := os.Stat(indexPath); os.IsNotExist(err) {
		initialHeader := "# Memory Index\n\nDurable project decisions, preferences, and architecture facts.\n\n"
		_ = os.WriteFile(indexPath, []byte(initialHeader), 0o644)
	}
	return &Store{
		baseDir:   baseDir,
		indexPath: indexPath,
		topicDir:  topicDir,
	}, nil
}

// AddFact records a durable fact to a topic file and links it in the MEMORY.md index.
func (s *Store) AddFact(topicName, fact string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	topicName = sanitizeTopic(topicName)
	if topicName == "" {
		topicName = "general"
	}
	fact = strings.TrimSpace(fact)
	if fact == "" {
		return nil
	}

	topicFile := filepath.Join(s.topicDir, topicName+".md")
	factLine := fmt.Sprintf("- [%s] %s\n", time.Now().Format("2006-01-02 15:04"), fact)

	f, err := os.OpenFile(topicFile, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return err
	}
	_, _ = f.WriteString(factLine)
	_ = f.Close()

	// Update MEMORY.md index
	return s.updateIndexLocked(topicName, fact)
}

func (s *Store) updateIndexLocked(topicName, fact string) error {
	data, err := os.ReadFile(s.indexPath)
	if err != nil && !os.IsNotExist(err) {
		return err
	}

	content := string(data)
	lines := strings.Split(content, "\n")

	// Append bullet referencing topic
	indexEntry := fmt.Sprintf("- **%s**: %s (see `topics/%s.md`)", topicName, fact, topicName)
	lines = append(lines, indexEntry)

	// Enforce <= 200 lines cap
	if len(lines) > MaxIndexLines {
		// Keep header lines (first 4) and prune oldest index lines
		header := lines[:4]
		trailing := lines[len(lines)-(MaxIndexLines-4):]
		lines = append(header, trailing...)
	}

	joined := strings.Join(lines, "\n")

	// Enforce <= 25 KB byte cap
	for len(joined) > MaxIndexBytes && len(lines) > 5 {
		// Drop the oldest non-header line
		lines = append(lines[:4], lines[5:]...)
		joined = strings.Join(lines, "\n")
	}

	return os.WriteFile(s.indexPath, []byte(joined), 0o644)
}

// Retrieve searches topics using keyword matching and returns <= 5 relevant topics.
func (s *Store) Retrieve(query string, maxTopics int) ([]Topic, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	if maxTopics <= 0 || maxTopics > MaxRetrievalTopics {
		maxTopics = MaxRetrievalTopics
	}

	entries, err := os.ReadDir(s.topicDir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}

	qTerms := strings.Fields(strings.ToLower(query))
	var scored []Topic

	for _, e := range entries {
		if e.IsDir() || !strings.HasSuffix(e.Name(), ".md") {
			continue
		}
		topicName := strings.TrimSuffix(e.Name(), ".md")
		filePath := filepath.Join(s.topicDir, e.Name())
		data, err := os.ReadFile(filePath)
		if err != nil {
			continue
		}

		text := string(data)
		textLower := strings.ToLower(text)
		nameLower := strings.ToLower(topicName)

		score := 0
		for _, term := range qTerms {
			if strings.Contains(nameLower, term) {
				score += 10
			}
			if strings.Contains(textLower, term) {
				score += 2
			}
		}

		if score > 0 {
			var facts []TopicFact
			sc := bufio.NewScanner(strings.NewReader(text))
			for sc.Scan() {
				line := strings.TrimSpace(sc.Text())
				if strings.HasPrefix(line, "- ") {
					facts = append(facts, TopicFact{
						Text: strings.TrimPrefix(line, "- "),
					})
				}
			}
			scored = append(scored, Topic{
				Name:  topicName,
				Path:  filePath,
				Score: score,
				Facts: facts,
			})
		}
	}

	sort.Slice(scored, func(i, j int) bool {
		return scored[i].Score > scored[j].Score
	})

	if len(scored) > maxTopics {
		scored = scored[:maxTopics]
	}

	return scored, nil
}

var memoryExtractionRegex = regexp.MustCompile(`(?i)(?:remember|note|decision|preference|architecture):\s*([^\n\.]+)`)

// ExtractFromTurn extracts durable facts from assistant text.
func (s *Store) ExtractFromTurn(text string) []string {
	matches := memoryExtractionRegex.FindAllStringSubmatch(text, -1)
	var extracted []string
	for _, m := range matches {
		if len(m) > 1 {
			fact := strings.TrimSpace(m[1])
			if fact != "" {
				extracted = append(extracted, fact)
				_ = s.AddFact("extracted", fact)
			}
		}
	}
	return extracted
}

// Consolidate compacts topic files by deduplicating facts and prunes MEMORY.md.
func (s *Store) Consolidate() error {
	s.mu.Lock()
	defer s.mu.Unlock()

	entries, err := os.ReadDir(s.topicDir)
	if err != nil {
		return err
	}

	for _, e := range entries {
		if e.IsDir() || !strings.HasSuffix(e.Name(), ".md") {
			continue
		}
		path := filepath.Join(s.topicDir, e.Name())
		data, err := os.ReadFile(path)
		if err != nil {
			continue
		}

		lines := strings.Split(string(data), "\n")
		seen := make(map[string]bool)
		var unique []string
		for _, l := range lines {
			trimmed := strings.TrimSpace(l)
			if trimmed == "" {
				continue
			}
			if !seen[trimmed] {
				seen[trimmed] = true
				unique = append(unique, trimmed)
			}
		}
		_ = os.WriteFile(path, []byte(strings.Join(unique, "\n")+"\n"), 0o644)
	}

	return nil
}

func sanitizeTopic(s string) string {
	s = strings.ToLower(strings.TrimSpace(s))
	reg := regexp.MustCompile(`[^a-z0-9_-]+`)
	return reg.ReplaceAllString(s, "-")
}
