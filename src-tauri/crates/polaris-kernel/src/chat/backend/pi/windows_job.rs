//! Reuse the existing windows-sys dependency for kernel-enforced parent lifetime.
use windows_sys::Win32::{Foundation::{CloseHandle, HANDLE}, System::{JobObjects::*, Threading::*}};
pub struct Job(HANDLE);
unsafe impl Send for Job {}
impl Job {
    pub fn attach(pid: u32) -> anyhow::Result<Self> {
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            anyhow::ensure!(!job.is_null(), "AgentdJobCreateFailed");
            let owned = Self(job);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            anyhow::ensure!(SetInformationJobObject(job, JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _, std::mem::size_of_val(&limits) as u32) != 0,
                "AgentdJobLimitsFailed");
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
            anyhow::ensure!(!process.is_null(), "AgentdJobOpenFailed");
            let assigned = AssignProcessToJobObject(job, process);
            CloseHandle(process);
            anyhow::ensure!(assigned != 0, "AgentdJobAssignFailed");
            Ok(owned)
        }
    }
}
impl Drop for Job { fn drop(&mut self) { unsafe { CloseHandle(self.0); } } }
