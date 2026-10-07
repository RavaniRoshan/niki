package protocol

import (
	"github.com/google/uuid"
)

type SessionId string
type TurnId string
type ItemId string
type ToolCallId string
type SubagentId string
type McpServerId string
type SkillId string

func newID() string { return uuid.Must(uuid.NewV7()).String() }

func NewSessionId() SessionId     { return SessionId(newID()) }
func NewTurnId() TurnId           { return TurnId(newID()) }
func NewItemId() ItemId           { return ItemId(newID()) }
func NewToolCallId() ToolCallId   { return ToolCallId(newID()) }
func NewSubagentId() SubagentId   { return SubagentId(newID()) }
func NewMcpServerId() McpServerId { return McpServerId(newID()) }
func NewSkillId() SkillId         { return SkillId(newID()) }
