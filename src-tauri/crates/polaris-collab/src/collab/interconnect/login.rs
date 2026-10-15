//! Login orchestration contract: shells supply I/O; ordering and failures are testable here.
use serde::Serialize;
use std::{future::Future, pin::Pin};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage { Register, Discover, Relay, Connect }

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub stage: Stage,
    pub state: &'static str,
    pub error: Option<String>,
    pub retryable: bool,
}

pub trait Steps {
    fn execute(&mut self, stage: Stage)
        -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>>;
    fn publish(&mut self, progress: &Progress);
}

/// No detached success: return the exact failed stage, and never execute later stages.
/// A shell may retry from Discover after durable enrollment has succeeded.
pub async fn run(steps: &mut impl Steps, start: Stage) -> Progress {
    let stages = [Stage::Register, Stage::Discover, Stage::Relay, Stage::Connect];
    for stage in stages.into_iter().skip_while(|stage| *stage != start) {
        let mut progress = Progress { stage, state: "running", error: None, retryable: false };
        steps.publish(&progress);
        if let Err(error) = steps.execute(stage).await {
            progress.state = "failed";
            progress.error = Some(error);
            progress.retryable = true;
            steps.publish(&progress);
            return progress;
        }
    }
    let progress = Progress {
        stage: Stage::Connect, state: "ready", error: None, retryable: false,
    };
    steps.publish(&progress);
    progress
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake { calls: Vec<Stage>, fail: Option<Stage>, published: Vec<Progress> }
    impl Steps for Fake {
        fn execute(&mut self, stage: Stage)
            -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
            Box::pin(async move {
                self.calls.push(stage);
                if self.fail == Some(stage) { Err("injected failure".into()) } else { Ok(()) }
            })
        }
        fn publish(&mut self, progress: &Progress) { self.published.push(progress.clone()); }
    }

    #[tokio::test]
    async fn every_failure_stops_before_later_side_effects() {
        let stages = [Stage::Register, Stage::Discover, Stage::Relay, Stage::Connect];
        for (index, stage) in stages.into_iter().enumerate() {
            let mut fake = Fake { calls: vec![], fail: Some(stage), published: vec![] };
            let out = run(&mut fake, Stage::Register).await;
            assert_eq!(fake.calls, stages[..=index]);
            assert_eq!(out.stage, stage);
            assert_eq!(out.state, "failed");
            assert_eq!(out.error.as_deref(), Some("injected failure"));
            assert!(out.retryable);
        }
    }

    #[tokio::test]
    async fn retry_after_enrollment_does_not_rotate_device_key_again() {
        let mut fake = Fake { calls: vec![], fail: Some(Stage::Relay), published: vec![] };
        run(&mut fake, Stage::Register).await;
        fake.calls.clear();
        fake.fail = None;
        let out = run(&mut fake, Stage::Discover).await;
        assert_eq!(fake.calls, [Stage::Discover, Stage::Relay, Stage::Connect]);
        assert_eq!(out.state, "ready");
        assert!(!out.retryable);
    }
}
