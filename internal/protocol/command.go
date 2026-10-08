package protocol

type CommandType string

const (
	CmdStartSession  CommandType = "start_session"
	CmdSubmitPrompt  CommandType = "submit_prompt"
	CmdInterruptTurn CommandType = "interrupt_turn"
	CmdCancelTurn    CommandType = "cancel_turn"
	CmdApproveTool   CommandType = "approve_tool"
	CmdRejectTool    CommandType = "reject_tool"
	CmdRefreshSkills CommandType = "refresh_skills"
	CmdRefreshMcp    CommandType = "refresh_mcp"
	CmdReloadConfig  CommandType = "reload_config"
	CmdCompact       CommandType = "compact"
	CmdShutdown      CommandType = "shutdown"
)

type EngineCommand struct {
	Type       CommandType `json:"type"`
	Prompt     string      `json:"prompt,omitempty"`
	CallID     ToolCallId  `json:"call_id,omitempty"`
	Approved   bool        `json:"approved,omitempty"`
	WorkingDir string      `json:"working_dir,omitempty"`
}
