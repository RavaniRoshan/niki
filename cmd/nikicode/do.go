package main

// `nikicode do` orchestration (G3): plan preview, multi-step execution,
// corrections with carried context, pronoun follow-ups, @-mentions, and
// journal recording for undo/redo. All dispatch is deterministic.

import (
	"context"
	"fmt"
	"strings"

	"github.com/RavaniRoshan/niki/internal/explain"
	"github.com/RavaniRoshan/niki/internal/git"
	"github.com/RavaniRoshan/niki/internal/intent"
	"github.com/RavaniRoshan/niki/internal/journal"
	"github.com/RavaniRoshan/niki/internal/mention"
	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/recipes"
	"github.com/RavaniRoshan/niki/internal/tools"
)

// doRun executes one natural-language input. planOnly prints the plan
// for recipe steps without running anything. Output always shows the
// plan before the results.
func doRun(reg *tools.Registry, guard *permissions.Guard, all []recipes.Recipe, cwd, input string, planOnly bool) (string, error) {
	in := strings.TrimSpace(input)
	if in == "" {
		return "", fmt.Errorf("say what to do, e.g. 'run the tests'")
	}
	lowered := strings.ToLower(in)
	if lowered == "undo" || lowered == "undo please" || lowered == "undo that" {
		msg, err := journal.Undo()
		if err != nil {
			return "", err
		}
		return msg, nil
	}
	if lowered == "redo" || lowered == "redo please" || lowered == "redo that" {
		return doRedo(reg, guard, all, cwd)
	}

	// @-mentions resolve before routing so every parser sees real paths.
	in = resolveMentions(cwd, in)

	// Corrections re-route with the last context carried over.
	carry := map[string]string{}
	if intent.IsCorrection(in) {
		in = intent.StripCorrection(in)
		if last, _ := journal.Last(); last != nil {
			for k, v := range last.Vars {
				carry[k] = v
			}
			// Bare vars ("name=cli") re-run the last recipe that
			// actually uses those vars (e.g. scaffold's {{name}}),
			// falling back to the most recent recipe.
			if intent.Route(all, in).Kind == intent.Unknown {
				newVars := intent.VarsFromInput(in)
				target := pickRecipeForVars(all, newVars)
				if target == "" {
					if last.Kind == "recipe" {
						target = last.Name
					} else {
						return "", fmt.Errorf("nothing to correct: say what to do instead")
					}
				}
				merged := map[string]string{}
				for k, v := range carry {
					merged[k] = v
				}
				for k, v := range intent.VarsFromInput(in) {
					merged[k] = v
				}
				text, effects, err := doRecipeStep(reg, guard, all, cwd, target, in, merged, planOnly)
				if err != nil {
					return text, err
				}
				if !planOnly {
					appendJournal(journal.Entry{Kind: "recipe", Name: target, Input: in, Dir: cwd, Vars: merged, Effects: effects, HeadBefore: git.Head(cwd), HeadAfter: git.Head(cwd)})
				}
				return text, nil
			}
		}
	}

	subject := lastSubject()
	steps := intent.SplitSteps(in)
	var stepOutputs []string
	for si, step := range steps {
		step = intent.SubstituteIt(step, subject)
		vars := intent.VarsFromInput(step)
		for k, v := range carry {
			if _, ok := vars[k]; !ok {
				vars[k] = v
			}
		}
		action := intent.Route(all, step)
		headBefore := git.Head(cwd)
		var text string
		var effects []recipes.FileEffect
		var err error
		switch action.Kind {
		case intent.Recipe:
			text, effects, err = doRecipeStep(reg, guard, all, cwd, action.Recipe.Name, step, vars, planOnly)
			if err == nil && !planOnly {
				appendJournal(journal.Entry{Kind: "recipe", Name: action.Recipe.Name, Input: step, Dir: cwd, Vars: vars, Effects: effects, HeadBefore: headBefore, HeadAfter: git.Head(cwd)})
			}
		case intent.Explain:
			ans := explain.AnswerQuestion(cwd, step)
			text = explain.Format(ans)
			if ans.Refused {
				err = fmt.Errorf("%s", text)
			} else {
				subject = explain.SubjectOf(step)
				if !planOnly {
					appendJournal(journal.Entry{Kind: "explain", Name: "explain", Input: step, Dir: cwd, Vars: vars, HeadBefore: headBefore, HeadAfter: git.Head(cwd), Extra: map[string]string{"subject": subject}})
				}
			}
		case intent.Git:
			extra := map[string]string{}
			if action.Op == intent.GitBranch {
				if prev, berr := git.CurrentBranch(cwd); berr == nil {
					if act, name, perr := parseBranchTarget(step); perr == nil && act == "create" {
						extra["undo"] = "branch"
						extra["name"] = name
						extra["prev"] = prev
					}
				}
			}
			text, err = runDoGitOp(cwd, intent.Action{Kind: intent.Git, Op: action.Op, Text: step, Vars: vars})
			if err == nil && !planOnly {
				appendJournal(journal.Entry{Kind: "git", Name: string(action.Op), Input: step, Dir: cwd, Vars: vars, HeadBefore: headBefore, HeadAfter: git.Head(cwd), Extra: extra})
			}
		default:
			err = fmt.Errorf("no routine matches %q", step)
		}
		carry = vars
		if err != nil {
			if len(stepOutputs) > 0 {
				return strings.Join(stepOutputs, "\n") + "\n", steppedError(si, len(steps), step, err)
			}
			return "", steppedError(si, len(steps), step, err)
		}
		stepOutputs = append(stepOutputs, fmt.Sprintf("### step %d: %s\n%s", si+1, step, text))
	}
	return strings.Join(stepOutputs, "\n"), nil
}

func steppedError(si, total int, step string, err error) error {
	if total > 1 {
		return fmt.Errorf("step %d of %d (%q) stopped: %v", si+1, total, step, err)
	}
	return err
}

// doRecipeStep prints the plan, then (unless planOnly) executes it,
// returning the combined text and the file effects for journaling.
func doRecipeStep(reg *tools.Registry, guard *permissions.Guard, all []recipes.Recipe, cwd, name, step string, vars map[string]string, planOnly bool) (string, []recipes.FileEffect, error) {
	var r *recipes.Recipe
	for i := range all {
		if all[i].Name == name {
			r = &all[i]
			break
		}
	}
	if r == nil {
		return "", nil, fmt.Errorf("recipe %q no longer exists", name)
	}
	v := map[string]string{"dir": cwd}
	for k, val := range vars {
		v[k] = val
	}
	var plan strings.Builder
	fmt.Fprintf(&plan, "Plan for %s (%d steps):", r.Name, len(r.Steps))
	for i, s := range r.Steps {
		raw := s.Raw
		for k, val := range v {
			raw = strings.ReplaceAll(raw, "{{"+k+"}}", val)
		}
		raw = strings.ReplaceAll(raw, "{{dir}}", cwd)
		fmt.Fprintf(&plan, "\n  %d. %s %s", i+1, s.Tool, raw)
	}
	if planOnly {
		return plan.String() + "\n(dry run: nothing executed)", nil, nil
	}
	rep, err := recipes.Execute(context.Background(), reg, guard, *r, v)
	if err != nil {
		return plan.String(), nil, err
	}
	var b strings.Builder
	b.WriteString(plan.String())
	for _, s := range rep.Steps {
		fmt.Fprintf(&b, "\n### %s\n%s", s.Tool, s.Output)
	}
	if rep.Refused {
		return b.String(), rep.Effects, fmt.Errorf("recipe %s stopped: %s", rep.Recipe, rep.Reason)
	}
	return b.String(), rep.Effects, nil
}

func appendJournal(e journal.Entry) {
	_ = journal.Append(e)
}

func lastSubject() string {
	if last, _ := journal.Last(); last != nil && last.Extra != nil {
		return last.Extra["subject"]
	}
	return ""
}

// doRedo re-executes the most recent undone entry as a fresh action
// (which appends its own journal entry).
func doRedo(reg *tools.Registry, guard *permissions.Guard, all []recipes.Recipe, cwd string) (string, error) {
	e, err := journal.RedoEntry()
	if err != nil {
		return "", err
	}
	vars := e.Vars
	if vars == nil {
		vars = map[string]string{}
	}
	vars["dir"] = cwd
	switch e.Kind {
	case "recipe":
		text, effects, err := doRecipeStep(reg, guard, all, cwd, e.Name, e.Input, vars, false)
		if err != nil {
			return text, err
		}
		appendJournal(journal.Entry{Kind: "recipe", Name: e.Name, Input: e.Input, Dir: cwd, Vars: vars, Effects: effects, HeadBefore: git.Head(cwd), HeadAfter: git.Head(cwd)})
		return text, nil
	case "git":
		action := intent.Route(all, e.Input)
		if action.Kind != intent.Git {
			action = intent.Action{Kind: intent.Git, Op: intent.GitOp(e.Name), Text: e.Input, Vars: vars}
		}
		headBefore := git.Head(cwd)
		text, err := runDoGitOp(cwd, action)
		if err != nil {
			return text, err
		}
		appendJournal(journal.Entry{Kind: "git", Name: string(action.Op), Input: e.Input, Dir: cwd, Vars: vars, HeadBefore: headBefore, HeadAfter: git.Head(cwd)})
		return text, nil
	case "explain":
		ans := explain.AnswerQuestion(cwd, e.Input)
		return explain.Format(ans), nil
	default:
		return "", fmt.Errorf("cannot redo %s %s", e.Kind, e.Name)
	}
}

// pickRecipeForVars finds a recipe whose steps reference one of the
// given var keys (e.g. {{name}}), preferring the most recently used.
// Empty when no recipe uses them.
func pickRecipeForVars(all []recipes.Recipe, vars map[string]string) string {
	if len(vars) == 0 {
		return ""
	}
	byName := map[string]recipes.Recipe{}
	for _, r := range all {
		byName[r.Name] = r
	}
	uses := func(r recipes.Recipe) bool {
		for _, s := range r.Steps {
			for k := range vars {
				if strings.Contains(s.Raw, "{{"+k+"}}") {
					return true
				}
			}
		}
		return false
	}
	if last, _ := journal.Last(); last != nil && last.Kind == "recipe" {
		if r, ok := byName[last.Name]; ok && uses(r) {
			return r.Name
		}
	}
	// Otherwise the most recent journal recipe using the vars.
	entries, _ := journal.Read()
	for i := len(entries) - 1; i >= 0; i-- {
		if entries[i].Kind != "recipe" || entries[i].Undone {
			continue
		}
		if r, ok := byName[entries[i].Name]; ok && uses(r) {
			return r.Name
		}
	}
	for _, r := range all {
		if uses(r) {
			return r.Name
		}
	}
	return ""
}

// resolveMentions substitutes @-mentions with resolved paths.
// Unresolvable mentions are left in place for downstream refusal.
func resolveMentions(cwd, input string) string {
	for _, m := range mention.Extract(input) {
		c, err := mention.Resolve(cwd, m)
		if err != nil {
			continue
		}
		input = strings.ReplaceAll(input, "@"+m, c.Path)
	}
	return input
}
