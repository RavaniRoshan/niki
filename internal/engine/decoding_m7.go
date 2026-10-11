package engine

// Decoding defaults per phase (M7): small models decode better with
// low temperature during tool/plan phases (where a wrong token breaks
// a tool call) and slightly higher temperature for free-form text.
// Every phase also sets an explicit output-token limit and stop
// sequences so a small model cannot ramble past the end of its turn or
// emit tool-call framing as prose.

// Phase is a turn phase with its own decoding defaults.
type Phase string

const (
	PhasePlan Phase = "plan" // planning / micro-steps
	PhaseTool Phase = "tool" // emitting a tool call
	PhaseText Phase = "text" // free-form assistant prose
)

// Decoding is the per-phase decoding configuration.
type Decoding struct {
	Temperature float64  // sampling temperature
	MaxTokens   int      // explicit output-token cap
	Stop        []string // stop sequences
}

// toolCallEndStop is the stop sequence that ends a tool-call emission.
// Built from runes so the source carries no literal markup token.
var toolCallEndStop = string([]rune{'<', '/', 't', 'o', 'o', 'l', '_', 'c', 'a', 'l', 'l', 's', '>'})

// DecodingFor returns the decoding defaults for a phase (M7).
// Tool and plan phases run cold (low temperature) so a small model
// does not corrupt a tool call or a step; text runs warmer. All phases
// carry an explicit output cap and stop sequences.
func DecodingFor(phase Phase) Decoding {
	switch phase {
	case PhaseTool:
		// Coldest: a tool call is structured; temperature 0 with a stop
		// on the tool-call terminator so the model cannot narrate past it.
		return Decoding{Temperature: 0.0, MaxTokens: 1024, Stop: []string{toolCallEndStop}}
	case PhasePlan:
		// Planning is also structured; cold, bounded, stop at plan end.
		return Decoding{Temperature: 0.1, MaxTokens: 2048, Stop: []string{toolCallEndStop}}
	case PhaseText:
		// Free-form prose runs warmer but is still bounded and stopped at
		// the end-of-turn marker.
		return Decoding{Temperature: 0.7, MaxTokens: 4096, Stop: []string{toolCallEndStop}}
	default:
		// Unknown phase: fail safe with the coldest, tightest defaults.
		return Decoding{Temperature: 0.0, MaxTokens: 1024, Stop: []string{toolCallEndStop}}
	}
}
