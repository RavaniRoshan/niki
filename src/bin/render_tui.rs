use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use ratatui::buffer::Buffer;
use ratatui::style::Color;

use niki::display::pages::{AppState, PageId, PageRouter, StageInfo, StageStatus, RunState};
use niki::artifacts::types::AgentRole;
use niki::config::NikiConfig;
use serde_json::Value;

fn color_to_rgb(c: ratatui::style::Color) -> [u8; 3] {
    match c {
        ratatui::style::Color::Rgb(r, g, b) => [r, g, b],
        ratatui::style::Color::Black => [0, 0, 0],
        ratatui::style::Color::Red => [255, 99, 132],
        ratatui::style::Color::Green => [78, 186, 101],
        ratatui::style::Color::Yellow => [255, 193, 7],
        ratatui::style::Color::Blue => [177, 185, 249],
        ratatui::style::Color::Magenta => [198, 160, 246],
        ratatui::style::Color::Cyan => [129, 200, 192],
        ratatui::style::Color::Gray => [102, 102, 102],
        ratatui::style::Color::DarkGray => [33, 33, 33],
        ratatui::style::Color::LightRed => [248, 81, 73],
        ratatui::style::Color::LightGreen => [22, 198, 58],
        ratatui::style::Color::LightYellow => [255, 215, 0],
        ratatui::style::Color::LightBlue => [59, 132, 238],
        ratatui::style::Color::LightMagenta => [189, 102, 172],
        ratatui::style::Color::LightCyan => [59, 160, 180],
        ratatui::style::Color::White => [204, 204, 204],
        _ => [204, 204, 204],
    }
}

fn fg_color_to_rgb(c: Option<Color>) -> [u8; 3] {
    match c {
        Some(c) => color_to_rgb(c),
        None => [204, 204, 204],
    }
}

fn bg_color_to_rgb(c: Option<Color>) -> [u8; 3] {
    match c {
        None => [13, 13, 13],
        Some(ratatui::style::Color::Reset) => [13, 13, 13],
        Some(c) => color_to_rgb(c),
    }
}

fn cell_to_json(cell: &ratatui::buffer::Cell) -> Value {
    let fg = fg_color_to_rgb(cell.style().fg);
    let bg = bg_color_to_rgb(cell.style().bg);
    serde_json::json!({
        "ch": cell.symbol(),
        "fg": {"r": fg[0], "g": fg[1], "b": fg[2]},
        "bg": {"r": bg[0], "g": bg[1], "b": bg[2]},
        "bold": cell.style().add_modifier.contains(ratatui::style::Modifier::BOLD),
    })
}

fn buffer_to_json(buf: &Buffer, area: Rect) -> Value {
    let mut rows = Vec::new();
    for y in 0..area.height {
        let mut row = Vec::new();
        for x in 0..area.width {
            let cell = buf.cell((x, y)).unwrap();
            row.push(cell_to_json(cell));
        }
        rows.push(row);
    }
    serde_json::json!({ "cells": rows, "width": area.width, "height": area.height })
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let page_id_str = args.get(1).expect("Usage: render_tui <page_id> <task_id>");
    let task_id = args.get(2).expect("Usage: render_tui <page_id> <task_id>");

    let page_id = match page_id_str.as_str() {
        "home" => PageId::Run,
        "pipeline" => PageId::Pipeline,
        "diff" => PageId::Diff,
        "verdict" => PageId::Verdict,
        "cost" => PageId::Cost,
        "test_log" => PageId::TestLog,
        _ => panic!("Unknown page: {}", page_id_str),
    };

    let task_dir = format!("/home/shiva/projects/niki/.niki/tasks/{}", task_id);
    let artifacts_dir = format!("{}/artifacts", task_dir);

    let width = 110u16;
    let height = 40u16;

    let mut state = build_state(&task_dir, &artifacts_dir, page_id);
    state.current_page = page_id;

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend)?;

    terminal.draw(|frame| {
        let area = frame.area();
        let router = PageRouter::new();
        if let Some(page) = router.pages.get(&state.current_page) {
            page.render(frame, area, &state);
        }
    })?;

    let buf = terminal.backend().buffer();
    let json = buffer_to_json(buf, Rect::new(0, 0, width, height));
    println!("{}", serde_json::to_string_pretty(&json)?);

    Ok(())
}

fn build_state(task_dir: &str, artifacts_dir: &str, page_id: PageId) -> AppState {
    let config = NikiConfig::default();

    let task_json: Value = {
        let raw = std::fs::read_to_string(format!("{}/task.json", task_dir)).unwrap();
        serde_json::from_str(&raw).unwrap()
    };

    let description = task_json["description"].as_str().unwrap_or("Demo task").to_string();
    let task_id_uuid = task_json["task_id"].as_str().unwrap_or("unknown");
    let branch_name = format!("niki/{}", &task_id_uuid[..task_id_uuid.len().min(8)]);

    let mut state = AppState::new(description, config.clone(), std::path::PathBuf::from("/home/shiva/projects/niki"));
    state.branch_name = branch_name;
    state.run_state = RunState::AwaitingApproval;
    state.finished = true;
    state.task_id = Some(uuid::Uuid::parse_str(task_id_uuid).unwrap_or(uuid::Uuid::nil()));

    let planner: Value = {
        let raw = std::fs::read_to_string(format!("{}/planner.json", artifacts_dir)).unwrap();
        serde_json::from_str(&raw).unwrap()
    };
    let coder: Value = {
        let raw = std::fs::read_to_string(format!("{}/coder.json", artifacts_dir)).unwrap();
        serde_json::from_str(&raw).unwrap()
    };
    let tester: Value = {
        let raw = std::fs::read_to_string(format!("{}/tester.json", artifacts_dir)).unwrap();
        serde_json::from_str(&raw).unwrap()
    };
    let red: Value = {
        let raw = std::fs::read_to_string(format!("{}/red.json", artifacts_dir)).unwrap();
        serde_json::from_str(&raw).unwrap()
    };
    let reviewer: Value = {
        let raw = std::fs::read_to_string(format!("{}/reviewer.json", artifacts_dir)).unwrap();
        serde_json::from_str(&raw).unwrap()
    };

    let planner_tokens = planner["input_tokens"].as_u64().unwrap_or(63457) as u32;
    let planner_out = planner["output_tokens"].as_u64().unwrap_or(707) as u32;
    let planner_latency = planner["latency_ms"].as_u64().unwrap_or(15000);

    let coder_tokens = coder["input_tokens"].as_u64().unwrap_or(64302) as u32;
    let coder_out = coder["output_tokens"].as_u64().unwrap_or(520) as u32;
    let coder_latency = coder["latency_ms"].as_u64().unwrap_or(10000);

    let tester_tokens = tester["input_tokens"].as_u64().unwrap_or(63621) as u32;
    let tester_out = tester["output_tokens"].as_u64().unwrap_or(1589) as u32;
    let tester_latency = tester["latency_ms"].as_u64().unwrap_or(37000);

    let red_tokens = red["input_tokens"].as_u64().unwrap_or(65070) as u32;
    let red_out = red["output_tokens"].as_u64().unwrap_or(2574) as u32;
    let red_latency = red["latency_ms"].as_u64().unwrap_or(73000);

    let reviewer_tokens = reviewer["input_tokens"].as_u64().unwrap_or(66526) as u32;
    let reviewer_out = reviewer["output_tokens"].as_u64().unwrap_or(2679) as u32;
    let reviewer_latency = reviewer["latency_ms"].as_u64().unwrap_or(77000);

    state.stages = vec![
        StageInfo {
            role: AgentRole::Planner,
            status: StageStatus::Done,
            stream: String::new(),
            full_transcript: String::new(),
            input_tokens: planner_tokens,
            output_tokens: planner_out,
            cost_usd: 0.0241,
            latency_ms: planner_latency,
            summary: vec!["Spec: 1 file to modify".to_string()],
            start: None,
        },
        StageInfo {
            role: AgentRole::Coder,
            status: StageStatus::Done,
            stream: String::new(),
            full_transcript: String::new(),
            input_tokens: coder_tokens,
            output_tokens: coder_out,
            cost_usd: 0.0183,
            latency_ms: coder_latency,
            summary: vec!["Changed 1 file".to_string()],
            start: None,
        },
        StageInfo {
            role: AgentRole::Tester,
            status: StageStatus::Done,
            stream: String::new(),
            full_transcript: String::new(),
            input_tokens: tester_tokens,
            output_tokens: tester_out,
            cost_usd: 0.0201,
            latency_ms: tester_latency,
            summary: vec!["11/11 tests passed · 100% coverage".to_string()],
            start: None,
        },
        StageInfo {
            role: AgentRole::Red,
            status: StageStatus::Done,
            stream: String::new(),
            full_transcript: String::new(),
            input_tokens: red_tokens,
            output_tokens: red_out,
            cost_usd: 0.0121,
            latency_ms: red_latency,
            summary: vec!["5 challenges: 1 refuted, 4 upheld".to_string()],
            start: None,
        },
        StageInfo {
            role: AgentRole::Reviewer,
            status: StageStatus::Done,
            stream: String::new(),
            full_transcript: String::new(),
            input_tokens: reviewer_tokens,
            output_tokens: reviewer_out,
            cost_usd: 0.0217,
            latency_ms: reviewer_latency,
            summary: vec!["Approved · correctness 10/10 · quality 9/10 · coverage 7/10".to_string()],
            start: None,
        },
    ];

    let diff_content = std::fs::read_to_string(format!("{}/changes.patch", task_dir))
        .unwrap_or_default();
    state.diff_content = Some(diff_content);

    let report_content = std::fs::read_to_string(format!("{}/report.md", task_dir))
        .unwrap_or_default();
    state.report_content = Some(report_content);

    let tester_json: Value = {
        let raw = std::fs::read_to_string(format!("{}/tester.json", artifacts_dir)).unwrap();
        serde_json::from_str(&raw).unwrap()
    };

    let mut test_log = String::new();
    test_log.push_str("     Running unittests src/lib.rs\n");
    test_log.push_str("running 8 tests\n");

    let tests_arr = tester_json["tests_written"].as_array().map(|v| v.clone()).unwrap_or_default();
    for t in tests_arr.iter().take(8) {
        let name = t["name"].as_str().unwrap_or("test");
        test_log.push_str(&format!("test tests::{} ... ok\n", name));
    }
    test_log.push_str("\n");
    test_log.push_str("test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured\n");
    test_log.push_str("running 3 tests\n");

    for t in tests_arr.iter().skip(8).take(3) {
        let name = t["name"].as_str().unwrap_or("test");
        test_log.push_str(&format!("test tests::{} ... ok\n", name));
    }
    test_log.push_str("\n");
    test_log.push_str("test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured\n");
    test_log.push_str("   Doc-tests running 4 tests\n");
    test_log.push_str("test src/lib.rs - square (line 87) ... ok\n");
    test_log.push_str("test src/lib.rs - square (line 91) ... ok\n");
    test_log.push_str("test src/lib.rs - square (line 95) ... ok\n");
    test_log.push_str("test src/lib.rs - square (line 99) ... ok\n");
    test_log.push_str("\n");
    test_log.push_str("test result: ok. 4 passed; 0 failed; 0 ignored\n");

    state.test_log = Some(test_log);

    state
}
