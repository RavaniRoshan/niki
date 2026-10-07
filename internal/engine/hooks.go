package engine

import "sync"

type HookPoint string

const (
	HookPreToolUse  HookPoint = "pre_tool_use"
	HookPostToolUse HookPoint = "post_tool_use"
	HookSessionStart HookPoint = "session_start"
	HookSessionEnd   HookPoint = "session_end"
)

type HookContext struct {
	Point    HookPoint
	ToolName string
	Payload  string
}

type HookFunc func(HookContext)

type HookRunner struct {
	mu    sync.RWMutex
	hooks map[HookPoint][]HookFunc
}

func NewHookRunner() *HookRunner {
	return &HookRunner{hooks: map[HookPoint][]HookFunc{}}
}

func (h *HookRunner) On(point HookPoint, fn HookFunc) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.hooks[point] = append(h.hooks[point], fn)
}

func (h *HookRunner) Fire(ctx HookContext) {
	h.mu.RLock()
	defer h.mu.RUnlock()
	for _, fn := range h.hooks[ctx.Point] {
		fn(ctx)
	}
}
