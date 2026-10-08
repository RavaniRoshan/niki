package intent

import (
	"testing"

	"github.com/RavaniRoshan/niki/internal/recipes"
)

func loadAll(t *testing.T) []recipes.Recipe {
	t.Helper()
	all, err := recipes.Discover("")
	if err != nil {
		t.Fatal(err)
	}
	return all
}

func TestRouteRecipesWin(t *testing.T) {
	all := loadAll(t)
	for input, want := range map[string]string{
		"run the tests please":  "test",
		"lint everything":       "lint",
		"build the project now": "build",
		"commit my changes":     "commit",
		"scaffold a worker":     "scaffold",
		"rename Old to New":      "refactor",
		"generate the docs":      "docs",
	} {
		a := Route(all, input)
		if a.Kind != Recipe || a.Recipe.Name != want {
			t.Fatalf("%q -> kind=%d recipe=%q, want %q", input, a.Kind, a.Recipe.Name, want)
		}
	}
}

func TestRouteGit(t *testing.T) {
	all := loadAll(t)
	cases := map[string]GitOp{
		"what is the status":            GitStatus,
		"show my working tree":          GitStatus,
		"create a branch called feat":   GitBranch,
		"switch to main":                GitBranch,
		"rebase onto main":              GitRebase,
		"resolve the merge conflict":    GitRebase,
		"blame line 10 of main.go":      GitBlame,
		"show recent commits":           GitLog,
		"review my staged changes":      GitReview,
		"write a changelog":             GitChangelog,
		"draft a pull request":          GitPRDraft,
		"commit the staged files":       GitCommit,
	}
	for input, want := range cases {
		a := Route(all, input)
		// "commit the staged files" hits the commit recipe first (gated flow).
		if input == "commit the staged files" {
			if a.Kind != Recipe || a.Recipe.Name != "commit" {
				t.Fatalf("%q should route to the commit recipe, got %+v", input, a)
			}
			continue
		}
		if a.Kind != Git || a.Op != want {
			t.Fatalf("%q -> kind=%d op=%q, want git/%q", input, a.Kind, a.Op, want)
		}
	}
}

func TestRouteExplain(t *testing.T) {
	all := loadAll(t)
	for _, input := range []string{
		"explain `RunTurn`",
		"what does the engine do?",
		"why does compaction exist?",
		"where is the session store?",
	} {
		a := Route(all, input)
		if a.Kind != Explain {
			t.Fatalf("%q -> kind=%d, want explain", input, a.Kind)
		}
	}
}

func TestRouteUnknown(t *testing.T) {
	all := loadAll(t)
	a := Route(all, "frobnicate the quux capacitor")
	if a.Kind != Unknown {
		t.Fatalf("nonsense routed to %+v", a)
	}
}

func TestVarsFromInput(t *testing.T) {
	vars := VarsFromInput("scaffold name=worker Name=Worker")
	if vars["name"] != "worker" || vars["Name"] != "Worker" {
		t.Fatalf("vars keep case: %v", vars)
	}
}

func TestSplitSteps(t *testing.T) {
	cases := map[string][]string{
		"scaffold name=a Name=A then build the project": {"scaffold name=a Name=A", "build the project"},
		"run the tests and then commit my changes":      {"run the tests", "commit my changes"},
		"status; log":                                   {"status", "log"},
		"just one thing":                                {"just one thing"},
	}
	for in, want := range cases {
		got := SplitSteps(in)
		if len(got) != len(want) {
			t.Fatalf("%q -> %v, want %v", in, got, want)
		}
		for i := range want {
			if got[i] != want[i] {
				t.Fatalf("%q step %d = %q, want %q", in, i, got[i], want[i])
			}
		}
	}
}

func TestCorrection(t *testing.T) {
	for _, in := range []string{"no, use main", "actually switch to dev", "I meant the other branch", "instead commit staged"} {
		if !IsCorrection(in) {
			t.Fatalf("%q should be a correction", in)
		}
	}
	if IsCorrection("commit my changes") || IsCorrection("nocturnal emissions") {
		t.Fatal("false correction")
	}
	if got := StripCorrection("no, use main"); got != "use main" {
		t.Fatalf("stripped = %q", got)
	}
}

func TestSubstituteIt(t *testing.T) {
	if got := SubstituteIt("explain it please", "RunTurn"); got != "explain RunTurn please" {
		t.Fatalf("got %q", got)
	}
	if got := SubstituteIt("commit my changes", "RunTurn"); got != "commit my changes" {
		t.Fatalf("false substitution: %q", got)
	}
	if got := SubstituteIt("explain it", ""); got != "explain it" {
		t.Fatalf("empty subject: %q", got)
	}
}
