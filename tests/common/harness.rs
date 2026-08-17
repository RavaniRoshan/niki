use crate::common::mock_llm::MockScriptBuilder;
use niki::config::NikiConfig;
use niki::display::agent_stream::AgenticDisplay;
use niki::orchestrator::pipeline::{Task, execute_pipeline};
use niki::sandbox::SandboxBackend;
use niki::sandbox::docker::ActiveContainers;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::common::fixture_repo::FixtureRepo;
use crate::common::mock_llm;

pub struct TestHarness {
    pub repo: FixtureRepo,
    pub config: NikiConfig,
    pub mock_script_path: PathBuf,
}

impl TestHarness {
    pub fn new() -> Self {
        Self::with_script(mock_llm::mock_script_for_happy_path)
    }

    pub fn with_script(script_gen: fn(&PathBuf) -> PathBuf) -> Self {
        let repo = crate::common::fixture_repo::create_fixture_repo();
        let dir = repo.dir.path();

        // Write a minimal git identity if not already set globally.
        let mock_path = dir.join(".niki-mock-script.json");
        let mock_script_path = script_gen(&mock_path);

        TestHarness {
            repo,
            config: NikiConfig::default(),
            mock_script_path,
        }
    }

    pub fn with_mock_builder<F>(self, f: F) -> Self
    where
        F: FnOnce(MockScriptBuilder) -> MockScriptBuilder,
    {
        let builder = f(MockScriptBuilder::new());
        let dir = self.repo.dir.path();
        let path = dir.join(".niki-mock-script.json");
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        std::fs::write(&path, builder.to_json_string()).unwrap();
        ();
        TestHarness {
            repo: self.repo,
            config: self.config,
            mock_script_path: path,
        }
    }

    pub fn with_worktree_backend(mut self) -> Self {
        self.config.docker.backend = SandboxBackend::Worktree;
        self
    }

    pub fn with_security_enabled(mut self) -> Self {
        self.config.security.enabled = true;
        self
    }

    pub fn with_parallel_enabled(mut self, count: u32) -> Self {
        self.config.parallel.enabled = true;
        self.config.parallel.coder_count = count;
        self
    }

    pub fn with_red_blue_disabled(mut self) -> Self {
        self.config.red_blue.enabled = false;
        self
    }

    pub fn with_mock_provider(mut self) -> Self {
        self.config.providers.insert(
            "mock".to_string(),
            niki::config::ProviderConfig {
                api_key: None,
                base_url: Some(self.mock_script_path.to_string_lossy().to_string()),
                default_model: "mock-planner".to_string(),
            },
        );
        self.config.agents.planner.provider = "mock".to_string();
        self.config.agents.planner.model = "mock-planner".to_string();
        self.config.agents.coder.provider = "mock".to_string();
        self.config.agents.coder.model = "mock-coder".to_string();
        self.config.agents.tester.provider = "mock".to_string();
        self.config.agents.tester.model = "mock-tester".to_string();
        self.config.agents.reviewer.provider = "mock".to_string();
        self.config.agents.reviewer.model = "mock-reviewer".to_string();
        self.config.agents.red.provider = "mock".to_string();
        self.config.agents.red.model = "mock-red".to_string();
        self.config.agents.synthesizer.provider = "mock".to_string();
        self.config.agents.synthesizer.model = "mock-synthesizer".to_string();
        self.config.agents.security_auditor.provider = "mock".to_string();
        self.config.agents.security_auditor.model = "mock-security_auditor".to_string();
        self
    }

    pub fn with_mock_provider_revised(mut self) -> Self {
        self.config.providers.insert(
            "mock".to_string(),
            niki::config::ProviderConfig {
                api_key: None,
                base_url: Some(self.mock_script_path.to_string_lossy().to_string()),
                default_model: "mock-planner".to_string(),
            },
        );
        self.config.agents.planner.provider = "mock".to_string();
        self.config.agents.planner.model = "mock-planner".to_string();
        self.config.agents.coder.provider = "mock".to_string();
        self.config.agents.coder.model = "mock-coder2".to_string();
        self.config.agents.tester.provider = "mock".to_string();
        self.config.agents.tester.model = "mock-tester2".to_string();
        self.config.agents.reviewer.provider = "mock".to_string();
        self.config.agents.reviewer.model = "mock-reviewer2".to_string();
        self.config.agents.red.provider = "mock".to_string();
        self.config.agents.red.model = "mock-red2".to_string();
        self
    }

    pub fn config(&self) -> &NikiConfig {
        &self.config
    }

    pub fn project_path(&self) -> PathBuf {
        self.repo.dir.path().to_path_buf()
    }

    pub async fn run_pipeline(&self) -> niki::orchestrator::pipeline::PipelineResult {
        let task = Task {
            id: Uuid::new_v4(),
            description: "Fix the off-by-one bug in paginate".to_string(),
            project_path: self.project_path(),
        };
        let mut display = AgenticDisplay::new();
        let containers: ActiveContainers = Arc::new(Mutex::new(Vec::new()));
        let cancel = Arc::new(AtomicBool::new(false));
        let task_dir = task
            .project_path
            .join(&self.config.general.output_dir)
            .join("tasks")
            .join(task.id.to_string());
        execute_pipeline(
            &task,
            &self.config,
            None,
            &mut display,
            containers,
            false,
            cancel,
            &task_dir,
        )
        .await
        .expect("pipeline should succeed")
    }

    pub async fn run_pipeline_dry(&self) -> niki::orchestrator::pipeline::PipelineResult {
        let task = Task {
            id: Uuid::new_v4(),
            description: "Fix the off-by-one bug in paginate".to_string(),
            project_path: self.project_path(),
        };
        let mut display = AgenticDisplay::new();
        let containers: ActiveContainers = Arc::new(Mutex::new(Vec::new()));
        let cancel = Arc::new(AtomicBool::new(false));
        let task_dir = task
            .project_path
            .join(&self.config.general.output_dir)
            .join("tasks")
            .join(task.id.to_string());
        execute_pipeline(
            &task,
            &self.config,
            None,
            &mut display,
            containers,
            true,
            cancel,
            &task_dir,
        )
        .await
        .expect("pipeline should succeed")
    }

    pub async fn run_pipeline_expect_fail(&self) -> anyhow::Error {
        let task = Task {
            id: Uuid::new_v4(),
            description: "Fix the off-by-one bug in paginate".to_string(),
            project_path: self.project_path(),
        };
        let mut display = AgenticDisplay::new();
        let containers: ActiveContainers = Arc::new(Mutex::new(Vec::new()));
        let cancel = Arc::new(AtomicBool::new(false));
        let task_dir = task
            .project_path
            .join(&self.config.general.output_dir)
            .join("tasks")
            .join(task.id.to_string());
        execute_pipeline(
            &task,
            &self.config,
            None,
            &mut display,
            containers,
            false,
            cancel,
            &task_dir,
        )
        .await
        .unwrap_err()
    }

    pub fn write_niki_toml(&self) {
        let dir = self.project_path();
        let toml = format!(
            r#"[docker]
backend = "worktree"

[providers.mock]
base_url = "{}"

[agents.planner]
provider = "mock"
model = "mock-planner"

[agents.coder]
provider = "mock"
model = "mock-coder"

[agents.tester]
provider = "mock"
model = "mock-tester"

[agents.reviewer]
provider = "mock"
model = "mock-reviewer"

[agents.red]
provider = "mock"
model = "mock-red"
"#,
            self.mock_script_path.display()
        );
        std::fs::write(dir.join("niki.toml"), toml).unwrap();
    }
}
