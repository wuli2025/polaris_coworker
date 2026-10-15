//! Supervisor-owned restore transaction: apply, verify, then release queued calls.
//! Configuration failures leave daemon health intact and retry on a later ping.
use super::{State, Process, Command, Empty, Id};
use super::super::{experts::{self, ExpertConfig}, protocol};
use serde_json::Value;

#[derive(Default)]
pub(super) struct Restore {
    waiting: Option<Id>,
    next: Option<Command>,
    expected: Option<ExpertConfig>,
    verifying: bool,
}
impl Restore {
    pub(super) fn busy(&self) -> bool { self.waiting.is_some() || self.next.is_some() }
}

impl State {
    pub(super) fn restore_experts(&mut self, actual: Option<ExpertConfig>) {
        if self.experts.busy() || self.status.lock().experts_synced == Some(true) { return; }
        let result = (|| -> anyhow::Result<()> {
            let Some(saved) = experts::load(&self.config.expert_settings)? else {
                self.expert_sync_finished(Ok(()));
                return Ok(());
            };
            let actual = actual.ok_or_else(|| anyhow::anyhow!("ExpertConfigReadUnsupported"))?
                .normalized(experts::prompt_limit()?)?;
            if actual == saved {
                self.expert_sync_finished(Ok(()));
            } else {
                self.experts.next = Some(Command::Apply(protocol::Apply {
                    config: Some(serde_json::json!({ "experts": saved })), credentials: None,
                }));
                self.experts.expected = Some(saved);
            }
            Ok(())
        })();
        if let Err(error) = result { self.expert_sync_finished(Err(error)); }
    }

    pub(super) fn send_expert_request(&mut self, process: &Process) -> anyhow::Result<()> {
        if let Some(command) = self.experts.next.take() {
            self.request(process, command, None)?;
            self.experts.waiting = Some(Id::Text(format!("rust-{}", self.counter)));
        }
        Ok(())
    }

    pub(super) fn expert_response(&mut self, id: &Id, result: &Result<Value, String>) -> bool {
        if self.experts.waiting.as_ref() != Some(id) { return false; }
        self.experts.waiting = None;
        let result = (|| -> anyhow::Result<()> {
            let value = result.as_ref().map_err(|error| anyhow::anyhow!("{error}"))?;
            if self.experts.verifying {
                let actual = protocol::check_ping(value.clone())?.experts
                    .ok_or_else(|| anyhow::anyhow!("ExpertConfigReadUnsupported"))?
                    .normalized(experts::prompt_limit()?)?;
                anyhow::ensure!(Some(&actual) == self.experts.expected.as_ref(), "ExpertConfigApplyMismatch");
                self.expert_sync_finished(Ok(()));
            } else {
                self.experts.verifying = true;
                self.experts.next = Some(Command::Ping(Empty {}));
            }
            Ok(())
        })();
        if let Err(error) = result { self.expert_sync_finished(Err(error)); }
        true
    }

    fn expert_sync_finished(&mut self, result: anyhow::Result<()>) {
        self.experts = Default::default();
        let mut status = self.status.lock();
        status.experts_synced = Some(result.is_ok());
        status.expert_sync_error = result.err().map(|error| error.to_string());
    }
}
