# NIKI UI/UX Refactor Plan: Claude Code & Kimi Code Interface Replication

**Date:** 2026-08-06  
**Depth:** Deep (production-grade, Product Hunt launch)  
**Target:** Full-fledged alternative interface matching Claude Code/Kimi Code quality

---

## Executive Summary

NIKI's current TUI is a viewer-only ratatui application with 11 navigable pages, no interactive input, no markdown rendering, and no split-pane layout. Claude Code uses React 18 + custom Ink fork + Yoga flexbox with 60fps double-buffered rendering, 144 UI components, and a fullscreen renderer with mouse support. Kimi Code uses pi-tui (a simpler component-based framework) with differential line-level rendering and synchronized output. Both share a conversational chat interface pattern with streaming markdown, permission modals, status bars, and slash command menus.

This plan transforms NIKI from a pipeline viewer into a full-fledged conversational AI coding assistant with an interface that surpasses both Claude Code and Kimi Code in visual polish and interaction quality.

---

## Part 1: Architecture Analysis

### 1.1 Claude Code Architecture (Reference)

| Layer | Technology | Key Detail |
|-------|-----------|------------|
| Runtime | Bun | Native TypeScript, build-time DCE via `feature()` |
| UI Framework | React 18 + Custom Ink fork | 60+ files in `src/ink/`, custom reconciler |
| Layout | Yoga (TypeScript port) | Flexbox: grow, shrink, padding, gap, alignment |
| Rendering | Packed Int32Array cells | 2 Int32 words per cell, pool-based interning |
| Diffing | Cell-level double-buffer | Damage rectangle, 30-50% byte reduction via optimizer |
| Frame Rate | 60fps via lodash throttle | 16ms interval, BSU/ESU synchronized output |
| State | AppState (300+ properties) | `useSyncExternalStore` pub/sub pattern |
| Streaming | Async generator | Yields events, `StreamingToolExecutor` for parallel tools |
| Components | 144 UI components, 85 hooks | REPL.tsx (~5000 lines) as main orchestrator |
| Themes | JSON files in `~/.claude/themes/` | 50+ color tokens, hot-reloadable |
| Input | Readline + Vim mode | Multiline, voice dictation, @ autocomplete, / commands |
| Mouse | Fullscreen mode only | Click-to-position, drag-select, Cmd+click URLs |
| Permissions | 7-stage pipeline, 6 modes | 51 React components for permission UI |

### 1.2 Kimi Code Architecture (Reference)

| Layer | Technology | Key Detail |
|-------|-----------|------------|
| Runtime | Node.js → single binary | TypeScript monorepo, pnpm |
| UI Framework | pi-tui (`@earendil-works/pi-tui`) | Custom framework by Mario Zechner |
| Component Model | `render(width): string[]` | No virtual DOM, no reconciler, no hooks |
| Rendering | Line-level differential | Only changed lines re-rendered |
| Output | CSI 2026 synchronized | Atomic flicker-free screen updates |
| Renderers | TuiMainScreen + TuiAltScreen | Main buffer (scrollback) or alternate buffer |
| Layout | VStack/HStack + ScrollView | basis, grow, shrink, minSize, maxSize |
| State | Imperative `tui-state.ts` | DI-based service composition |
| Colors | `Theme` singleton class | `fg()`, `boldFg()`, `dimFg()` helpers with chalk |
| Input | @ autocomplete, / commands | ! shell mode, Ctrl-S inject, Ctrl-G external editor |
| Components | 15 built-in | Text, Editor, Markdown, Loader, SelectList, etc. |
| Themes | JSON in `~/.kimi-code/themes/` | `{ name, base, colors }` schema |

### 1.3 NIKI Current State

| Layer | Technology | Gap |
|-------|-----------|-----|
| Runtime | Rust (2024 edition) | No change needed |
| UI Framework | ratatui 0.29 + crossterm | Needs major extension |
| Rendering | Immediate-mode, 30fps cap | Needs synchronized output, higher fps |
| State | `AppState` in pages/mod.rs | Needs reactive pub/sub pattern |
| Input | None (viewer-only) | Needs full input system |
| Streaming | DisplayEvent channel | Needs real-time token streaming |
| Themes | Dark/Light/Auto (735 lines) | Needs expansion to 50+ tokens |
| Layout | Full-page navigation | Needs split-pane conversational layout |
| Components | 11 pages + overlays | Needs chat, input, markdown, status bar |

---

## Part 2: Visual Design Specification

### 2.1 Color System (Dual Theme)

Based on Kimi Code's proven palette (confirmed from source) with NIKI brand accents:

#### Dark Theme

| Token | Hex | Usage |
|-------|-----|-------|
| `bg` | `#121111` | Main background |
| `bg.surface` | `#1A1A1A` | Cards, panels |
| `bg.elevated` | `#242424` | Modals, overlays |
| `primary` | `#4FA8FF` | Links, inline code, focused elements, spinners |
| `accent` | `#5BC0BE` | Approval prefix, secondary actions |
| `text` | `#E0E0E0` | Default body text |
| `text.strong` | `#F5F5F5` | Bold/emphasized text |
| `text.dim` | `#888888` | Thinking blocks, hints, descriptions |
| `text.muted` | `#6B6B6B` | Counters, scroll info, URLs |
| `border` | `#5A5A5A` | Pane borders, horizontal rules |
| `border.focus` | `#E8A838` | Focused element border |
| `success` | `#4EC87E` | Checkmarks, enabled states, test pass |
| `warning` | `#E8A838` | Auto/yolo badges, stale markers |
| `error` | `#E85454` | Error messages, failed tests |
| `diff.added` | `#4EC87E` | Added lines |
| `diff.removed` | `#E85454` | Removed lines |
| `diff.added.strong` | `#7AD99B` | Added word highlights |
| `diff.removed.strong` | `#F08585` | Removed word highlights |
| `diff.gutter` | `#6B6B6B` | Line numbers |
| `diff.meta` | `#888888` | Hunk headers |
| `role.user` | `#FFCB6B` | User message bullets |
| `role.assistant` | `#4FA8FF` | Assistant message label |
| `role.system` | `#888888` | System messages |
| `shell` | `#BD93F9` | Shell mode border/prompt |
| `claude` | `#BD93F9` | NIKI brand accent (spinner, logo) |

#### Light Theme

| Token | Hex | Usage |
|-------|-----|-------|
| `bg` | `#FDFCFC` | Main background |
| `bg.surface` | `#F0F0F0` | Cards, panels |
| `bg.elevated` | `#FFFFFF` | Modals, overlays |
| `primary` | `#1565C0` | Links, inline code |
| `accent` | `#00838F` | Secondary actions |
| `text` | `#1A1A1A` | Default body text |
| `text.strong` | `#1A1A1A` | Bold/emphasized |
| `text.dim` | `#454545` | Hints, descriptions |
| `text.muted` | `#5F5F5F` | Counters, URLs |
| `border` | `#737373` | Pane borders |
| `border.focus` | `#92660A` | Focused element |
| `success` | `#0E7A38` | Checkmarks |
| `warning` | `#92660A` | Badges |
| `error` | `#B91C1C` | Errors |
| `diff.added` | `#0E7A38` | Added lines |
| `diff.removed` | `#B91C1C` | Removed lines |
| `role.user` | `#9A4A00` | User bullets |
| `shell` | `#7C3AED` | Shell mode |

All text tokens ≥ 4.5:1 contrast against background (WCAG AA). Chrome tokens ≥ 3:1.

### 2.2 Typography & Spacing

- **Font:** Terminal's native monospace font (no control over this)
- **Line height:** 1 line per terminal row (standard)
- **Padding:** 1 cell horizontal inside panels, 0 vertical
- **Panel borders:** Unicode box-drawing: `╭─╮│╰─╯` (rounded corners)
- **Horizontal rules:** `─` repeated across width in `border` color
- **Section headers:** Bold + `text.strong` color
- **Code blocks:** `primary` color for inline, syntax-highlighted for blocks
- **List bullets:** `•` in `text` color for unordered, numbered for ordered

### 2.3 Icons & Visual Patterns

| Element | Pattern | Source |
|---------|---------|--------|
| Logo | Block characters `▐█▛█▛█▌` / `▐█████▌` in `primary` | Kimi Code |
| Spinner | Moon loader `◑ ◐ ◑ ◐` rotating in `claude` color | Kimi Code |
| Status dot | `●` colored by state (green=running, yellow=waiting, red=error) | Claude Code |
| Check mark | `✓` in `success` color | Both |
| Cross mark | `✗` in `error` color | Both |
| Arrow | `→` in `text.dim` for stage transitions | NIKI current |
| Bullet | `⏺` for tool calls, `⎿` for tool results | Claude Code |
| Star | `✦` for banner prefix in `primary` | Kimi Code |
| Progress | `[████████░░] 80%` in `primary`/`text.muted` | Both |

---

## Part 3: Layout Architecture

### 3.1 Main Layout (Conversational Mode)

```
┌─────────────────────────────────────────────────────────────┐
│  ╭─ NIKI ──────────────────────────────────────────────╮    │
│  │  ✦ Welcome to NIKI                                  │    │
│  │  Send /help for help information.                   │    │
│  │                                                     │    │
│  │  Directory: /path/to/project                        │    │
│  │  Session:   session_uuid                            │    │
│  │  Model:     claude-sonnet-4-20250514                │    │
│  │  Version:   0.2.0                                   │    │
│  ╰─────────────────────────────────────────────────────╯    │
│                                                             │
│  ● user                                                     │
│  Add a GET /health endpoint                                 │
│                                                             │
│  ◈ planner                                                 │
│  Planning implementation...                                 │
│  ├── Reading src/main.rs                                    │
│  ├── Reading src/routes/mod.rs                              │
│  └── Spec: 2 files to modify                               │
│                                                             │
│  ⟠ coder                                                   │
│  Editing src/routes/mod.rs                                  │
│  ```diff                                                    │
│  + #[get("/health")]                                        │
│  + async fn health() -> impl IntoResponse {                 │
│  +     Json(serde_json::json!({ "status": "ok" }))         │
│  + }                                                        │
│  ```                                                        │
│                                                             │
│  ◉ tester                                                  │
│  Running 8 tests...                                         │
│  ✓ 8/8 tests passed                                        │
│                                                             │
│  ◆ reviewer                                                │
│  Verdict: Approved                                          │
│  ├── correctness: 10/10                                    │
│  ├── quality: 8/10                                         │
│  └── coverage: 10/10                                       │
│                                                             │
│  ─────────────────────────────────────────────────────────  │
│  ● NIKI                                                    │
│  Branch: niki/6d281d6d · Verdict: Approved                 │
│  Changes: 1 file modified · Tests: 8/8 passed              │
│  Cost: $0.042 · Tokens: 12,847                             │
│  [Space] Pause  [Tab] Pages  [Ctrl+P] Commands  [/] Menu   │
├─────────────────────────────────────────────────────────────┤
│ > _                                          context: 2.1%  │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 Page Navigation (Tab Mode)

When user presses `Tab`, the view switches to page mode with a tab bar:

```
┌─────────────────────────────────────────────────────────────┐
│  [Pipeline] [Agents] [Diff] [Verdict] [Cost] [Artifacts]   │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  (Page content renders here)                                │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│  [Space] Pause  [Tab] Chat  [Ctrl+P] Commands  [/] Menu    │
└─────────────────────────────────────────────────────────────┘
```

### 3.3 Permission Modal (Overlay)

```
┌─────────────────────────────────────────────────────────────┐
│  (Conversation dimmed behind)                               │
│                                                             │
│  ╭─ Permission Required ────────────────────────────────╮   │
│  │                                                      │   │
│  │  The agent wants to run:                              │   │
│  │  $ cargo test                                         │   │
│  │                                                      │   │
│  │  ● Allow once    ○ Allow always    ○ Deny            │   │
│  │                                                      │   │
│  │  [Enter] Confirm  [Esc] Deny                         │   │
│  ╰──────────────────────────────────────────────────────╯   │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ > _                                          context: 2.1%  │
└─────────────────────────────────────────────────────────────┘
```

### 3.4 Slash Command Menu (Overlay)

```
┌─────────────────────────────────────────────────────────────┐
│  (Conversation dimmed behind)                               │
│                                                             │
│  ╭─ Commands ───────────────────────────────────────────╮   │
│  │  /help          Show help information                 │   │
│  │  /compact       Compact conversation context          │   │
│  │  /clear         Clear conversation                    │   │
│  │  /cost          Show cost breakdown                   │   │
│  │  /diff          Show current diff                     │   │
│  │  /model         Switch model                          │   │
│  │  /pipeline      Show pipeline status                  │   │
│  │  /tui           Switch TUI mode                       │   │
│  │  /theme         Cycle theme                           │   │
│  ╰──────────────────────────────────────────────────────╯   │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ > /_                                         context: 2.1%  │
└─────────────────────────────────────────────────────────────┘
```

---

## Part 4: Implementation Plan

### Phase 1: Foundation (Days 1-3)

**Goal:** Replace the current page-based TUI with a conversational chat layout.

#### 1.1 New Module Structure

```
src/display/
├── mod.rs              (existing - keep as facade)
├── engine.rs           (NEW - rendering engine)
├── state.rs            (NEW - reactive state management)
├── input.rs            (NEW - input handling)
├── chat/
│   ├── mod.rs
│   ├── message.rs      (message rendering)
│   ├── streaming.rs    (streaming text display)
│   ├── markdown.rs     (markdown parser + renderer)
│   └── code_block.rs   (syntax-highlighted code blocks)
├── components/
│   ├── mod.rs
│   ├── status_bar.rs   (bottom status bar)
│   ├── input_box.rs    (text input with cursor)
│   ├── welcome.rs      (welcome banner)
│   ├── spinner.rs      (animated spinner)
│   ├── permission.rs   (permission modal)
│   ├── command_menu.rs (slash command menu)
│   ├── autocomplete.rs (@ file autocomplete)
│   └── progress.rs     (progress indicators)
├── layout/
│   ├── mod.rs
│   ├── chat_layout.rs  (main conversational layout)
│   ├── page_layout.rs  (tab-based page layout)
│   └── overlay.rs      (modal overlay system)
├── theme.rs            (existing - expand tokens)
├── pages/              (existing - keep for tab mode)
│   └── ...
└── tui.rs              (existing - refactor to use new engine)
```

#### 1.2 Rendering Engine (`engine.rs`)

Replace the current 30fps capped render loop with a high-performance engine:

```rust
pub struct RenderEngine {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    back_buffer: CellBuffer,
    front_buffer: CellBuffer,
    damage_rect: Option<DamageRect>,
    frame_interval: Duration, // 16ms for 60fps
    synchronized_output: bool, // CSI 2026 support
}

impl RenderEngine {
    pub fn render(&mut self, state: &AppState) -> Result<()> {
        // 1. Render state to back buffer
        self.render_to_buffer(state)?;
        
        // 2. Diff against front buffer
        let changes = self.diff_buffers();
        
        // 3. Apply synchronized output if supported
        if self.synchronized_output {
            self.write_csi_2026_start()?;
        }
        
        // 4. Write only changed cells
        self.write_changes(&changes)?;
        
        // 5. Swap buffers
        std::mem::swap(&mut self.front_buffer, &mut self.back_buffer);
        
        if self.synchronized_output {
            self.write_csi_2026_end()?;
        }
        
        Ok(())
    }
}
```

**Key decisions:**
- Use `crossterm::event::EventStream` for async input (already in deps)
- Implement cell-level diffing similar to Claude Code's approach
- Detect CSI 2026 support at startup (already in `tui.rs`)
- Target 60fps for streaming, 30fps for idle

#### 1.3 Reactive State (`state.rs`)

Replace the current `apply_event()` pattern with a reactive store:

```rust
pub struct Store {
    state: AppState,
    subscribers: Vec<Box<dyn Fn(&AppState)>>,
    event_tx: mpsc::UnboundedSender<StoreEvent>,
    event_rx: mpsc::UnboundedReceiver<StoreEvent>,
}

impl Store {
    pub fn subscribe(&mut self, f: impl Fn(&AppState) + 'static) {
        self.subscribers.push(Box::new(f));
    }
    
    pub fn dispatch(&mut self, event: StoreEvent) {
        // Apply event to state
        self.apply_event(event);
        
        // Notify subscribers
        for subscriber in &self.subscribers {
            subscriber(&self.state);
        }
        
        // Trigger re-render
        self.request_render();
    }
}
```

**State shape (expanded from current 14-field `AppState`):**

```rust
pub struct AppState {
    // Navigation
    pub view: ViewMode, // Chat | Page(PageId)
    pub page: PageId,
    
    // Conversation
    pub messages: Vec<Message>,
    pub streaming_message: Option<StreamingMessage>,
    pub input_state: InputState,
    
    // Pipeline
    pub pipeline: PipelineState,
    pub stages: Vec<StageInfo>,
    
    // UI State
    pub theme: ThemeMode,
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    pub show_command_menu: bool,
    pub show_permission_modal: bool,
    pub show_help: bool,
    
    // Context
    pub context_usage: f64, // 0.0 - 1.0
    pub token_count: usize,
    pub cost: f64,
    
    // Config
    pub config: NikiConfig,
}
```

#### 1.4 Input System (`input.rs`)

Full input handling with cursor management:

```rust
pub struct InputState {
    pub buffer: String,
    pub cursor_pos: usize,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub mode: InputMode, // Insert | Command | Shell
    pub autocomplete: Option<AutocompleteState>,
}

pub enum InputMode {
    Insert,      // Normal typing
    Command,     // / command menu active
    Shell,       // ! shell mode
    VimNormal,   // Vim normal mode (optional)
    VimVisual,   // Vim visual mode (optional)
}

impl InputState {
    pub fn handle_key(&mut self, key: KeyEvent) -> InputAction {
        match self.mode {
            InputMode::Insert => self.handle_insert_key(key),
            InputMode::Command => self.handle_command_key(key),
            InputMode::Shell => self.handle_shell_key(key),
            _ => InputAction::None,
        }
    }
}
```

**Key bindings (matching Claude Code/Kimi Code):**

| Key | Action |
|-----|--------|
| `Enter` | Submit input / confirm selection |
| `Escape` | Close menu/modal, cancel autocomplete |
| `Ctrl+C` | Cancel current operation |
| `Ctrl+L` | Clear screen |
| `Ctrl+P` | Command palette |
| `Ctrl+T` | Cycle theme |
| `Tab` | Switch to page mode / autocomplete |
| `Shift+Tab` | Previous autocomplete |
| `Up/Down` | History navigation / menu navigation |
| `Ctrl+A/E` | Beginning/end of line |
| `Ctrl+W` | Delete word backward |
| `Ctrl+U` | Delete to beginning |
| `Ctrl+K` | Delete to end |
| `@` | Trigger file autocomplete |
| `/` | Trigger command menu (when input empty) |
| `!` | Enter shell mode (when input empty) |

### Phase 2: Chat Interface (Days 4-7)

**Goal:** Implement the conversational chat display with streaming markdown.

#### 2.1 Message Rendering (`chat/message.rs`)

```rust
pub enum Message {
    User {
        content: String,
        timestamp: DateTime<Utc>,
    },
    Assistant {
        content: String,
        role: AgentRole, // planner, coder, tester, reviewer
        timestamp: DateTime<Utc>,
        tool_calls: Vec<ToolCall>,
        thinking: Option<String>,
    },
    System {
        content: String,
        level: SystemLevel, // info, warning, error
    },
}

pub fn render_message(message: &Message, width: usize, theme: &Theme) -> Vec<Line> {
    match message {
        Message::User { content, .. } => {
            // ● user (gold bullet)
            // Content text
            vec![
                Line::from(Span::styled("● ", theme.role_user())),
                Line::from(Span::styled("user", theme.role_user().add_modifier(Modifier::BOLD))),
                Line::from(""),
                render_markdown(content, width, theme),
            ]
        }
        Message::Assistant { content, role, tool_calls, .. } => {
            // ◈ planner (role-colored icon + label)
            // Content with streaming support
            // Tool calls with collapsible output
            let icon = role_icon(role);
            let color = role_color(role);
            let mut lines = vec![
                Line::from(Span::styled(format!("{} ", icon), color)),
                Line::from(Span::styled(role_name(role), color.add_modifier(Modifier::BOLD))),
                Line::from(""),
            ];
            lines.extend(render_markdown(content, width, theme));
            
            // Tool calls
            for tool_call in tool_calls {
                lines.extend(render_tool_call(tool_call, width, theme));
            }
            
            lines
        }
        _ => vec![],
    }
}
```

#### 2.2 Streaming Display (`chat/streaming.rs`)

Real-time token rendering with incomplete markdown handling:

```rust
pub struct StreamingMessage {
    pub buffer: String,
    pub rendered_lines: Vec<Line>,
    pub last_render_pos: usize,
    pub incomplete_code_fence: bool,
    pub incomplete_bold: bool,
    pub incomplete_list: bool,
}

impl StreamingMessage {
    pub fn push_token(&mut self, token: &str, width: usize, theme: &Theme) {
        self.buffer.push_str(token);
        
        // Re-render only the new content
        let new_content = &self.buffer[self.last_render_pos..];
        
        // Handle incomplete markdown
        let rendered = render_streaming_markdown(
            new_content,
            width,
            theme,
            &mut self.incomplete_code_fence,
            &mut self.incomplete_bold,
            &mut self.incomplete_list,
        );
        
        self.rendered_lines.extend(rendered);
        self.last_render_pos = self.buffer.len();
    }
    
    pub fn finalize(&mut self, width: usize, theme: &Theme) {
        // Re-render entire message as static markdown
        self.rendered_lines = render_markdown(&self.buffer, width, theme);
    }
}
```

#### 2.3 Markdown Rendering (`chat/markdown.rs`)

Full markdown parser with syntax highlighting:

```rust
use pulldown_cmark::{Parser, Tag, Event};
use syntect::easy::HighlightLines;
use syntect::parsing::SyntaxSet;
use syntect::highlighting::ThemeSet;

pub fn render_markdown(input: &str, width: usize, theme: &Theme) -> Vec<Line> {
    let parser = Parser::new(input);
    let mut lines = Vec::new();
    let mut current_line = Line::default();
    
    for event in parser {
        match event {
            Event::Start(Tag::Heading(level)) => {
                // Bold + color based on level
                current_line.push_span(Span::styled(
                    "#".repeat(level as usize) + " ",
                    theme.text_dim(),
                ));
            }
            Event::Start(Tag::CodeBlock(lang)) => {
                // Syntax-highlighted code block
                lines.extend(render_code_block(&content, lang, width, theme));
            }
            Event::Start(Tag::List(_)) => {
                current_line.push_span(Span::styled("• ", theme.text()));
            }
            Event::Code(code) => {
                // Inline code in primary color
                current_line.push_span(Span::styled(
                    format!("`{}`", code),
                    theme.primary(),
                ));
            }
            Event::Text(text) => {
                current_line.push_span(Span::styled(text.to_string(), theme.text()));
            }
            _ => {}
        }
    }
    
    lines
}
```

**Code block rendering with syntax highlighting:**

```rust
pub fn render_code_block(
    code: &str,
    lang: &str,
    width: usize,
    theme: &Theme,
) -> Vec<Line> {
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let syntax = syntax_set
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
    
    let mut h = HighlightLines::new(syntax, &theme.code_theme());
    let mut lines = Vec::new();
    
    // Border top
    lines.push(Line::from(Span::styled(
        "─".repeat(width),
        theme.border(),
    )));
    
    for line in code.lines() {
        let highlighted = h.highlight_line(line, &syntax_set).unwrap();
        let mut rendered = Line::default();
        for (style, text) in highlighted {
            rendered.push_span(Span::styled(
                text.to_string(),
                convert_style(style),
            ));
        }
        lines.push(rendered);
    }
    
    // Border bottom
    lines.push(Line::from(Span::styled(
        "─".repeat(width),
        theme.border(),
    )));
    
    lines
}
```

### Phase 3: Components (Days 8-12)

**Goal:** Build all interactive components matching Claude Code/Kimi Code quality.

#### 3.1 Status Bar (`components/status_bar.rs`)

```
● NIKI ─── Plan mode ─── claude-sonnet-4 ─── background: 2 tasks ─── context: 2.1% (12,847/200,000)
```

```rust
pub fn render_status_bar(state: &AppState, width: usize, theme: &Theme) -> Line {
    let mut spans = vec![
        Span::styled("● ", theme.claude()),
        Span::styled("NIKI", theme.claude().add_modifier(Modifier::BOLD)),
        Span::styled(" ─── ", theme.border()),
    ];
    
    // Mode badge
    if state.pipeline.mode == PipelineMode::Plan {
        spans.push(Span::styled("Plan mode ", theme.primary()));
        spans.push(Span::styled("─── ", theme.border()));
    }
    
    // Model
    spans.push(Span::styled(
        format!("{} ", state.config.model),
        theme.text_dim(),
    ));
    spans.push(Span::styled("─── ", theme.border()));
    
    // Background tasks
    if state.background_tasks > 0 {
        spans.push(Span::styled(
            format!("background: {} tasks ", state.background_tasks),
            theme.primary(),
        ));
        spans.push(Span::styled("─── ", theme.border()));
    }
    
    // Context usage
    let pct = (state.context_usage * 100.0) as u32;
    let color = if pct > 80 { theme.error() }
                else if pct > 60 { theme.warning() }
                else { theme.success() };
    spans.push(Span::styled(
        format!("context: {}% ({}/{})", pct, state.token_count, state.context_limit),
        color,
    ));
    
    Line::from(spans)
}
```

#### 3.2 Input Box (`components/input_box.rs`)

```
> Add a GET /health endpoint_
```

```rust
pub fn render_input_box(state: &InputState, width: usize, theme: &Theme) -> Vec<Line> {
    let mut lines = vec![];
    
    // Prompt border
    lines.push(Line::from(Span::styled(
        format!("╭{}╮", "─".repeat(width - 2)),
        theme.prompt_border(),
    )));
    
    // Input line with cursor
    let prompt = match state.mode {
        InputMode::Shell => Span::styled("! ", theme.shell()),
        _ => Span::styled("> ", theme.primary()),
    };
    
    let mut input_spans = vec![prompt];
    let before_cursor = &state.buffer[..state.cursor_pos];
    let cursor_char = state.buffer[state.cursor_pos..].chars().next();
    let after_cursor = &state.buffer[state.cursor_pos + cursor_char.map_or(0, |c| c.len_utf8())..];
    
    input_spans.push(Span::styled(before_cursor.to_string(), theme.text()));
    input_spans.push(Span::styled(
        cursor_char.map_or(" ", |c| c.to_string()),
        theme.primary().add_modifier(Modifier::REVERSED),
    ));
    input_spans.push(Span::styled(after_cursor.to_string(), theme.text()));
    
    lines.push(Line::from(input_spans));
    
    // Bottom border
    lines.push(Line::from(Span::styled(
        format!("╰{}╯", "─".repeat(width - 2)),
        theme.prompt_border(),
    )));
    
    lines
}
```

#### 3.3 Spinner (`components/spinner.rs`)

```rust
pub struct Spinner {
    frames: Vec<&'static str>,
    index: usize,
    color: Style,
}

impl Spinner {
    pub fn moon() -> Self {
        Self {
            frames: vec!["◑", "◒", "◓", "◔"],
            index: 0,
            color: Style::default(), // Set from theme
        }
    }
    
    pub fn tick(&mut self) -> &str {
        let frame = self.frames[self.index];
        self.index = (self.index + 1) % self.frames.len();
        frame
    }
    
    pub fn render(&self) -> Span {
        Span::styled(self.tick(), self.color)
    }
}
```

#### 3.4 Permission Modal (`components/permission.rs`)

```rust
pub fn render_permission_modal(
    request: &PermissionRequest,
    selected: usize,
    width: usize,
    theme: &Theme,
) -> Vec<Line> {
    let modal_width = 60.min(width - 4);
    let padding = (width - modal_width) / 2;
    let pad = " ".repeat(padding);
    
    let mut lines = vec![];
    
    // Scrim (dim background)
    lines.push(Line::from(""));
    
    // Top border
    lines.push(Line::from(Span::styled(
        format!("{}╭─ Permission Required ─{}╮", pad, "─".repeat(modal_width - 24)),
        theme.border(),
    )));
    
    // Content
    lines.push(Line::from(Span::styled(
        format!("{}│", pad),
        theme.border(),
    )));
    lines.push(Line::from(Span::styled(
        format!("{}│  The agent wants to run:", pad),
        theme.text(),
    )));
    lines.push(Line::from(Span::styled(
        format!("{}│  $ {}", pad, request.command),
        theme.primary(),
    )));
    lines.push(Line::from(Span::styled(
        format!("{}│", pad),
        theme.border(),
    )));
    
    // Options
    let options = ["Allow once", "Allow always", "Deny"];
    for (i, option) in options.iter().enumerate() {
        let marker = if i == selected { "●" } else { "○" };
        let style = if i == selected { theme.primary() } else { theme.text() };
        lines.push(Line::from(Span::styled(
            format!("{}│  {} {}", pad, marker, option),
            style,
        )));
    }
    
    lines.push(Line::from(Span::styled(
        format!("{}│", pad),
        theme.border(),
    )));
    
    // Bottom border
    lines.push(Line::from(Span::styled(
        format!("{}╰{}╯", pad, "─".repeat(modal_width - 2)),
        theme.border(),
    )));
    
    // Key hints
    lines.push(Line::from(Span::styled(
        format!("{}[Enter] Confirm  [Esc] Deny", pad),
        theme.text_dim(),
    )));
    
    lines
}
```

#### 3.5 Command Menu (`components/command_menu.rs`)

```rust
pub fn render_command_menu(
    commands: &[Command],
    selected: usize,
    filter: &str,
    width: usize,
    theme: &Theme,
) -> Vec<Line> {
    let filtered: Vec<_> = commands.iter()
        .filter(|c| c.name.contains(filter) || c.description.contains(filter))
        .collect();
    
    let mut lines = vec![];
    let menu_width = 50.min(width - 4);
    let padding = (width - menu_width) / 2;
    let pad = " ".repeat(padding);
    
    // Header
    lines.push(Line::from(Span::styled(
        format!("{}╭─ Commands ──╮", pad),
        theme.border(),
    )));
    
    // Commands
    for (i, cmd) in filtered.iter().enumerate() {
        let marker = if i == selected { "●" } else { " " };
        let style = if i == selected { theme.primary() } else { theme.text() };
        lines.push(Line::from(vec![
            Span::styled(format!("{} {} ", pad, marker), style),
            Span::styled(format!("{:<16}", cmd.name), style.add_modifier(Modifier::BOLD)),
            Span::styled(cmd.description.clone(), theme.text_dim()),
        ]));
    }
    
    // Footer
    lines.push(Line::from(Span::styled(
        format!("{}╰──────────────╯", pad),
        theme.border(),
    )));
    
    lines
}
```

### Phase 4: Layout System (Days 13-15)

**Goal:** Implement the split-pane layout with page navigation.

#### 4.1 Chat Layout (`layout/chat_layout.rs`)

```rust
pub fn render_chat_layout(
    state: &AppState,
    area: Rect,
    theme: &Theme,
) -> Vec<Line> {
    let mut lines = vec![];
    
    // Messages area (scrollable)
    let messages_height = area.height.saturating_sub(5); // Input + status bar
    
    let visible_messages = get_visible_messages(
        &state.messages,
        state.scroll_offset,
        messages_height,
        area.width,
        theme,
    );
    
    for line in visible_messages {
        lines.push(line);
    }
    
    // Fill remaining space
    while lines.len() < messages_height as usize {
        lines.push(Line::from(""));
    }
    
    // Separator
    lines.push(Line::from(Span::styled(
        "─".repeat(area.width as usize),
        theme.border(),
    )));
    
    // Status bar
    lines.push(render_status_bar(state, area.width as usize, theme));
    
    // Input box
    lines.extend(render_input_box(&state.input_state, area.width as usize, theme));
    
    lines
}
```

#### 4.2 Page Layout (`layout/page_layout.rs`)

```rust
pub fn render_page_layout(
    state: &AppState,
    area: Rect,
    theme: &Theme,
) -> Vec<Line> {
    let mut lines = vec![];
    
    // Tab bar
    let tabs = render_tab_bar(state.page, area.width as usize, theme);
    lines.push(tabs);
    lines.push(Line::from(Span::styled(
        "─".repeat(area.width as usize),
        theme.border(),
    )));
    
    // Page content
    let page = state.pages.get(&state.page).unwrap();
    let content = page.render(&state.page_state, area.width as usize, theme);
    lines.extend(content);
    
    // Footer
    lines.push(Line::from(Span::styled(
        "─".repeat(area.width as usize),
        theme.border(),
    )));
    lines.push(Line::from(Span::styled(
        format!("[Space] Pause  [Tab] Chat  [Ctrl+P] Commands  [/] Menu"),
        theme.text_dim(),
    )));
    
    lines
}
```

#### 4.3 Overlay System (`layout/overlay.rs`)

```rust
pub fn render_overlay(
    base: Vec<Line>,
    overlay: Vec<Line>,
    area: Rect,
) -> Vec<Line> {
    // Dim the base content
    let dimmed: Vec<Line> = base.into_iter()
        .map(|line| line.dimmed())
        .collect();
    
    // Calculate overlay position (centered)
    let overlay_height = overlay.len();
    let overlay_width = overlay.iter()
        .map(|line| line.width())
        .max()
        .unwrap_or(0);
    
    let start_y = (area.height as usize - overlay_height) / 2;
    let start_x = (area.width as usize - overlay_width) / 2;
    
    // Merge overlay onto dimmed base
    let mut result = dimmed;
    for (i, line) in overlay.into_iter().enumerate() {
        if start_y + i < result.len() {
            // Insert overlay line at position
            result[start_y + i] = merge_line(
                &result[start_y + i],
                &line,
                start_x,
            );
        }
    }
    
    result
}
```

### Phase 5: Integration (Days 16-18)

**Goal:** Wire everything together with the existing pipeline system.

#### 5.1 Event Bridge

```rust
// Adapt existing DisplayEvent to new Store events
impl From<DisplayEvent> for StoreEvent {
    fn from(event: DisplayEvent) -> Self {
        match event {
            DisplayEvent::Banner { .. } => StoreEvent::ShowWelcome,
            DisplayEvent::StageStart { role, .. } => StoreEvent::StageStarted(role),
            DisplayEvent::StageToken { role, token } => StoreEvent::TokenReceived(role, token),
            DisplayEvent::StageDone { role, .. } => StoreEvent::StageCompleted(role),
            DisplayEvent::DiffContent { diff } => StoreEvent::DiffReceived(diff),
            DisplayEvent::CostJson { cost } => StoreEvent::CostUpdated(cost),
            DisplayEvent::Final { branch, verdict, .. } => StoreEvent::TaskCompleted {
                branch,
                verdict,
            },
            _ => StoreEvent::Other(event),
        }
    }
}
```

#### 5.2 Main Event Loop

```rust
pub async fn run_tui(config: NikiConfig) -> Result<()> {
    let mut engine = RenderEngine::new()?;
    let mut store = Store::new(config);
    let mut input_handler = InputHandler::new();
    let mut event_stream = crossterm::event::EventStream::new();
    
    // Initial render
    engine.render(&store.state)?;
    
    loop {
        tokio::select! {
            // Terminal input
            Some(event) = event_stream.next() => {
                match event {
                    Event::Key(key) => {
                        let action = input_handler.handle_key(key, &store.state);
                        match action {
                            InputAction::Submit(input) => {
                                store.dispatch(StoreEvent::UserInput(input));
                            }
                            InputAction::Navigate(page) => {
                                store.dispatch(StoreEvent::NavigateTo(page));
                            }
                            InputAction::Quit => break,
                            _ => {}
                        }
                    }
                    Event::Mouse(mouse) => {
                        store.dispatch(StoreEvent::MouseEvent(mouse));
                    }
                    Event::Resize(_, _) => {
                        engine.resize()?;
                    }
                    _ => {}
                }
            }
            
            // Pipeline events
            Some(event) = store.event_rx.recv() => {
                store.dispatch(event.into());
            }
            
            // Render timer
            _ = tokio::time::sleep(Duration::from_millis(16)) => {
                if store.needs_render() {
                    engine.render(&store.state)?;
                    store.clear_render_flag();
                }
            }
        }
    }
    
    Ok(())
}
```

### Phase 6: Polish & Testing (Days 19-21)

**Goal:** Visual polish, animation, and comprehensive testing.

#### 6.1 Animations

```rust
// Shimmer effect for spinner
pub fn render_shimmer(text: &str, progress: f64, theme: &Theme) -> Line {
    let chars: Vec<char> = text.chars().collect();
    let mut spans = vec![];
    
    for (i, &ch) in chars.iter().enumerate() {
        let pos = (i as f64 / chars.len() as f64 + progress) % 1.0;
        let color = interpolate_color(
            theme.claude(),
            theme.claude_shimmer(),
            pos,
        );
        spans.push(Span::styled(ch.to_string(), color));
    }
    
    Line::from(spans)
}

// Rainbow effect for ultrathink
pub fn render_rainbow(text: &str, progress: f64) -> Line {
    let colors = [
        "#FF0000", "#FF7F00", "#FFFF00", "#00FF00",
        "#0000FF", "#4B0082", "#9400D3",
    ];
    
    let chars: Vec<char> = text.chars().collect();
    let mut spans = vec![];
    
    for (i, &ch) in chars.iter().enumerate() {
        let pos = (i as f64 / chars.len() as f64 + progress) % 1.0;
        let color_idx = (pos * colors.len() as f64) as usize;
        let color = Color::from_hex(colors[color_idx % colors.len()]);
        spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
    }
    
    Line::from(spans)
}
```

#### 6.2 Testing Strategy

```rust
// Unit tests for each component
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_render_message_user() {
        let message = Message::User {
            content: "Hello".to_string(),
            timestamp: Utc::now(),
        };
        let theme = Theme::dark();
        let lines = render_message(&message, 80, &theme);
        assert_eq!(lines.len(), 3); // bullet, label, content
    }
    
    #[test]
    fn test_render_status_bar() {
        let state = AppState::default();
        let theme = Theme::dark();
        let line = render_status_bar(&state, 80, &theme);
        assert!(line.width() <= 80);
    }
    
    #[test]
    fn test_render_permission_modal() {
        let request = PermissionRequest {
            command: "cargo test".to_string(),
            tool: "bash".to_string(),
        };
        let theme = Theme::dark();
        let lines = render_permission_modal(&request, 0, 80, &theme);
        assert!(lines.len() > 5);
    }
    
    #[test]
    fn test_input_cursor_movement() {
        let mut input = InputState::new();
        input.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        assert_eq!(input.cursor_pos, 3);
        
        input.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert_eq!(input.cursor_pos, 2);
    }
}
```

---

## Part 5: Dependency Changes

### 5.1 New Dependencies

```toml
[dependencies]
# Markdown parsing
pulldown-cmark = "0.12"

# Syntax highlighting
syntect = "5"  # Already in deps

# Async event stream
crossterm = { version = "0.28", features = ["event-stream"] }

# Clipboard
arboard = "3"

# File path completion
glob = "0.3"  # Already in deps

# Time
chrono = "0.4"  # Already in deps
```

### 5.2 Remove/Replace

```toml
# Remove (replaced by new input system)
# indicatif = "0.17"  # Progress bars replaced by custom components

# Keep (still used)
ratatui = "0.29"
crossterm = "0.28"
console = "0.15"
textwrap = "0.16"
syntect = "5"
```

---

## Part 6: Migration Strategy

### 6.1 Backward Compatibility

- Keep `--tui` flag behavior (opt-in)
- Keep non-TUI output mode (for CI/piped)
- Keep existing `DisplayEvent` enum (adapt, don't replace)
- Keep `AgenticDisplay` bridge pattern

### 6.2 Feature Flags

```rust
// In config or env
NIKI_TUI_MODE=chat      # New conversational mode (default)
NIKI_TUI_MODE=pages     # Old page-based mode
NIKI_TUI_FULLSCREEN=1   # Alternate screen buffer
```

### 6.3 Rollback Plan

If the new TUI has issues:
1. Set `NIKI_TUI_MODE=pages` to revert to old behavior
2. Remove `--tui` flag to use non-TUI output
3. Old page implementations remain in `pages/` directory

---

## Part 7: Quality Checklist

### Visual Quality
- [ ] Dark theme matches Kimi Code's proven palette
- [ ] Light theme maintains WCAG AA contrast ratios
- [ ] Unicode box-drawing characters render correctly
- [ ] Syntax highlighting works for Rust, Python, JavaScript, TypeScript
- [ ] Spinner animation is smooth (60fps)
- [ ] Shimmer effect works on terminals supporting truecolor
- [ ] Modal overlays dim background correctly
- [ ] Status bar updates in real-time

### Interaction Quality
- [ ] Input cursor moves correctly with arrow keys
- [ ] History navigation works with Up/Down arrows
- [ ] @ autocomplete shows file paths
- [ ] / command menu filters in real-time
- [ ] Tab switches between chat and page mode
- [ ] Escape closes all menus/modals
- [ ] Ctrl+C cancels current operation
- [ ] Mouse click positions cursor in input
- [ ] Mouse wheel scrolls conversation
- [ ] Drag-to-select copies to clipboard

### Performance Quality
- [ ] Streaming tokens render without flicker
- [ ] Long conversations don't degrade performance
- [ ] Memory usage stays flat (no leaks)
- [ ] Startup time < 100ms
- [ ] Frame rate stays at 60fps during streaming

### Compatibility Quality
- [ ] Works in tmux with mouse mode
- [ ] Works in VS Code integrated terminal
- [ ] Works in iTerm2
- [ ] Works in Kitty
- [ ] Works in Ghostty
- [ ] Works over SSH
- [ ] Falls back gracefully on unsupported terminals

---

## Part 8: Timeline & Milestones

| Day | Milestone | Deliverable |
|-----|-----------|-------------|
| 1-2 | Foundation | `engine.rs`, `state.rs`, basic render loop |
| 3 | Input System | `input.rs`, cursor management, key bindings |
| 4-5 | Chat Display | Message rendering, streaming support |
| 6-7 | Markdown | Parser, syntax highlighting, code blocks |
| 8-9 | Components | Status bar, spinner, input box |
| 10-11 | Overlays | Permission modal, command menu |
| 12 | Autocomplete | @ file completion, / command filtering |
| 13-14 | Layout | Chat layout, page layout, overlay system |
| 15 | Integration | Wire with existing pipeline system |
| 16-17 | Theme Expansion | 50+ tokens, custom themes, hot-reload |
| 18 | Mouse Support | Click, drag, scroll, URL opening |
| 19-20 | Polish | Animations, shimmer, rainbow effects |
| 21 | Testing | Unit tests, integration tests, manual QA |

---

## Part 9: Risk Mitigation

### Risk 1: ratatui Performance Limitations
**Mitigation:** ratatui is proven at scale (Netflix, OpenAI Codex CLI). If cell-level diffing is needed, implement custom buffer management on top of ratatui's `Buffer` type.

### Risk 2: Markdown Rendering Complexity
**Mitigation:** Use `pulldown-cmark` for parsing (battle-tested), `syntect` for highlighting (already a dependency). Start with basic markdown, add features incrementally.

### Risk 3: Terminal Compatibility
**Mitigation:** Detect terminal capabilities at startup (CSI 2026, truecolor, mouse). Gracefully degrade: no synchronized output → flicker-free rendering, no truecolor → 256-color fallback.

### Risk 4: Input System Bugs
**Mitigation:** Start with simple readline-style input. Add vim mode as optional feature flag. Test with unit tests for cursor movement, history, and autocomplete.

### Risk 5: Integration with Existing Pipeline
**Mitigation:** Keep `DisplayEvent` enum and `AgenticDisplay` bridge. Adapt events to new `StoreEvent` system. Old behavior preserved via `NIKI_TUI_MODE=pages`.

---

## Part 10: Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Startup time | < 100ms | `time niki run --tui` |
| Frame rate | 60fps | Terminal FPS counter |
| Memory usage | < 50MB | `ps aux` |
| Input latency | < 16ms | Keypress to display |
| Streaming throughput | > 100 tokens/sec | Token counter |
| Test coverage | > 80% | `cargo test` |
| Visual match | 95%+ | Side-by-side comparison with Claude Code |

---

## Appendix A: File Change Summary

| File | Action | Lines Changed |
|------|--------|---------------|
| `src/display/mod.rs` | Modify | ~50 |
| `src/display/engine.rs` | Create | ~300 |
| `src/display/state.rs` | Create | ~200 |
| `src/display/input.rs` | Create | ~400 |
| `src/display/chat/mod.rs` | Create | ~50 |
| `src/display/chat/message.rs` | Create | ~250 |
| `src/display/chat/streaming.rs` | Create | ~150 |
| `src/display/chat/markdown.rs` | Create | ~300 |
| `src/display/chat/code_block.rs` | Create | ~150 |
| `src/display/components/mod.rs` | Create | ~20 |
| `src/display/components/status_bar.rs` | Create | ~100 |
| `src/display/components/input_box.rs` | Create | ~150 |
| `src/display/components/spinner.rs` | Create | ~80 |
| `src/display/components/permission.rs` | Create | ~120 |
| `src/display/components/command_menu.rs` | Create | ~100 |
| `src/display/components/autocomplete.rs` | Create | ~150 |
| `src/display/components/progress.rs` | Create | ~80 |
| `src/display/layout/mod.rs` | Create | ~20 |
| `src/display/layout/chat_layout.rs` | Create | ~150 |
| `src/display/layout/page_layout.rs` | Create | ~100 |
| `src/display/layout/overlay.rs` | Create | ~100 |
| `src/display/theme.rs` | Modify | ~200 |
| `src/display/tui.rs` | Modify | ~100 |
| `src/display/pages/mod.rs` | Modify | ~50 |
| `Cargo.toml` | Modify | ~10 |
| **Total** | | **~3,500** |

---

## Appendix B: Key Design Decisions

1. **Keep ratatui** (don't switch to a custom framework like Kimi Code's pi-tui) — ratatui is proven, has ecosystem support, and NIKI already uses it.

2. **Implement cell-level diffing** (inspired by Claude Code) — even though ratatui doesn't do this natively, we can implement it on top of `Buffer` for flicker-free rendering.

3. **Use pulldown-cmark** for markdown (not a custom parser) — battle-tested, handles edge cases, supports all markdown features.

4. **Keep DisplayEvent bridge** (don't rewrite pipeline) — the existing pipeline is solid; we only change the display layer.

5. **Dual mode** (chat + pages) — chat for conversational use, pages for pipeline inspection. Users can switch with Tab.

6. **Theme-first design** — every color goes through the theme system. No hardcoded colors anywhere.

7. **Graceful degradation** — always work, even on terminals without truecolor, synchronized output, or mouse support.

---

*This plan is designed to be executed autonomously. All fallback options are documented. The implementation should follow the phase order strictly, with each phase building on the previous one.*
