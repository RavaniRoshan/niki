package contextwin

import "strings"

// StaticPrefix is the cacheable prompt prefix: system identity, tool rules,
// and skill catalog only. No session-varying data (no cwd, no time, no model)
// may appear here or the provider prompt cache fragments.
func StaticPrefix(systemPrompt string, skillCatalog string) string {
	return systemPrompt + "\n\n" + skillCatalog
}

// DynamicContext is the per-turn, session-varying suffix.
func DynamicContext(cwd, branch, model, gitStatus string) string {
	var b strings.Builder
	b.WriteString("<context>\n")
	b.WriteString("cwd: " + cwd + "\n")
	b.WriteString("branch: " + branch + "\n")
	b.WriteString("model: " + model + "\n")
	b.WriteString("git_status: " + gitStatus + "\n")
	b.WriteString("</context>\n")
	return b.String()
}

// Assemble joins the static prefix and the dynamic context with a marked
// boundary. The static prefix must never contain session-varying fields.
func Assemble(systemPrompt, skillCatalog, dynamic string) string {
	return StaticPrefix(systemPrompt, skillCatalog) + "\n\n---\n\n" + dynamic
}
