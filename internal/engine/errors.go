package engine

import "fmt"

type ErrorKind string

const (
	ErrProvider  ErrorKind = "provider"
	ErrTool      ErrorKind = "tool"
	ErrPermission ErrorKind = "permission"
	ErrCancelled ErrorKind = "cancelled"
	ErrInternal  ErrorKind = "internal"
)

type Error struct {
	Kind    ErrorKind
	Message string
	Err     error
}

func (e *Error) Error() string {
	if e.Err != nil {
		return fmt.Sprintf("%s: %s: %v", e.Kind, e.Message, e.Err)
	}
	return fmt.Sprintf("%s: %s", e.Kind, e.Message)
}

func (e *Error) Unwrap() error { return e.Err }

func WrapProvider(err error) *Error  { return &Error{Kind: ErrProvider, Message: "provider failure", Err: err} }
func WrapTool(err error) *Error      { return &Error{Kind: ErrTool, Message: "tool failure", Err: err} }
func WrapCancelled() *Error          { return &Error{Kind: ErrCancelled, Message: "cancelled"} }
