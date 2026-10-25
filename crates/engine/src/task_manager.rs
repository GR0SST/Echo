//! Owned task data. CEF handles stay on its UI thread and never cross the facade.
use cef::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskKind {
    Browser,
    Gpu,
    Utility,
    Renderer,
    Extension,
    Worker,
    Other,
}
#[derive(Clone, Debug)]
pub struct TaskRecord {
    pub id: i64,
    pub title: String,
    pub kind: TaskKind,
    pub killable: bool,
    pub memory_bytes: Option<u64>,
    pub cpu_percent: Option<f64>,
    /// OS process identity, never the CEF task ID. None when the backend cannot expose it.
    pub process_id: Option<u32>,
    /// Measured process network throughput in bytes per second, not a lifetime byte count.
    pub network_bytes_per_second: Option<u64>,
}
/// Thread-affine facade: implementations own their sampling/termination capabilities.
/// The UI consumes the same interface for the retained CEF monitor and test samplers.
pub trait TaskSampler {
    fn snapshot(&self) -> Result<Vec<TaskRecord>, String>;
    fn end(&self, id: i64) -> Result<(), String>;
}
/// Preserve backend order inside a process group. Unknown identities remain separate:
/// equal metrics, titles or adjacent CEF IDs do not establish process identity.
pub fn group_tasks(rows: Vec<TaskRecord>) -> Vec<Vec<TaskRecord>> {
    let mut groups: Vec<Vec<TaskRecord>> = Vec::new();
    let mut indexes: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for row in rows {
        let index = row
            .process_id
            .filter(|id| *id != 0)
            .and_then(|id| indexes.get(&id).copied());
        if let Some(index) = index {
            groups[index].push(row);
        } else {
            if let Some(id) = row.process_id.filter(|id| *id != 0) {
                indexes.insert(id, groups.len());
            }
            groups.push(vec![row]);
        }
    }
    groups
}
fn kind(value: sys::cef_task_type_t) -> TaskKind {
    use sys::cef_task_type_t::*;
    match value {
        CEF_TASK_TYPE_BROWSER => TaskKind::Browser,
        CEF_TASK_TYPE_GPU => TaskKind::Gpu,
        CEF_TASK_TYPE_UTILITY | CEF_TASK_TYPE_ZYGOTE | CEF_TASK_TYPE_SANDBOX_HELPER => {
            TaskKind::Utility
        }
        CEF_TASK_TYPE_RENDERER | CEF_TASK_TYPE_GUEST => TaskKind::Renderer,
        CEF_TASK_TYPE_EXTENSION => TaskKind::Extension,
        CEF_TASK_TYPE_DEDICATED_WORKER
        | CEF_TASK_TYPE_SHARED_WORKER
        | CEF_TASK_TYPE_SERVICE_WORKER => TaskKind::Worker,
        _ => TaskKind::Other,
    }
}
const PROCESS_IDENTITY: &str = "Echo.TaskProcessIdentity";
#[derive(Default)]
struct ProcessIdentities(std::collections::HashMap<i32, (u64, u32)>);
impl ProcessIdentities {
    fn record(&mut self, browser: i32, epoch: u64, pid: u32) {
        self.0.insert(browser, (epoch, pid));
    }
    fn retain_current(&mut self, epoch: impl Fn(i32) -> Option<u64>) {
        self.0
            .retain(|browser, (generation, _)| epoch(*browser) == Some(*generation));
    }
}
thread_local! { static PROCESSES: std::cell::RefCell<ProcessIdentities> = std::cell::RefCell::new(ProcessIdentities::default()); }
fn decode_pid(text: &str) -> Option<u32> {
    if text.is_empty() || text.len() > 10 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<u32>().ok().filter(|pid| *pid != 0)
}
/// Runs in the renderer helper, with no page-visible JavaScript callback.
pub(crate) fn report_process(frame: &Frame) {
    if frame.is_main() == 0 || frame.is_valid() == 0 {
        return;
    }
    let Some(mut message) = process_message_create(Some(&CefString::from(PROCESS_IDENTITY))) else {
        return;
    };
    let Some(args) = message.argument_list() else {
        return;
    };
    args.set_string(
        0,
        Some(&CefString::from(std::process::id().to_string().as_str())),
    );
    frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
}
pub(crate) fn receive_process(
    browser: Option<&Browser>,
    frame: Option<&Frame>,
    source: ProcessId,
    message: Option<&ProcessMessage>,
) -> bool {
    let Some(message) =
        message.filter(|m| CefString::from(&m.name()).to_string() == PROCESS_IDENTITY)
    else {
        return false;
    };
    if source != ProcessId::RENDERER {
        return true;
    }
    let (Some(browser), Some(frame)) = (browser, frame) else {
        return true;
    };
    if frame.is_main() == 0 || frame.is_valid() == 0 {
        return true;
    }
    let Some(current) = browser.main_frame() else {
        return true;
    };
    if CefString::from(&current.identifier()).to_string()
        != CefString::from(&frame.identifier()).to_string()
    {
        return true;
    }
    let Some(args) = message
        .argument_list()
        .filter(|a| a.size() == 1 && a.get_type(0) == ValueType::STRING)
    else {
        return true;
    };
    let Some(pid) = decode_pid(&CefString::from(&args.string(0)).to_string()) else {
        return true;
    };
    if let Some(epoch) = crate::site_data::document_epoch(browser.identifier()) {
        PROCESSES.with_borrow_mut(|processes| processes.record(browser.identifier(), epoch, pid));
    }
    true
}
pub(crate) fn forget_process(browser: i32) {
    PROCESSES.with_borrow_mut(|p| {
        p.0.remove(&browser);
    });
}
fn process_tasks(manager: &TaskManager) -> std::collections::HashMap<i64, u32> {
    let entries = PROCESSES.with_borrow_mut(|processes| {
        processes.retain_current(crate::site_data::document_epoch);
        processes
            .0
            .iter()
            .map(|(browser, (_, pid))| (*browser, *pid))
            .collect::<Vec<_>>()
    });
    entries
        .into_iter()
        .filter_map(|(browser, pid)| {
            let task = manager.task_id_for_browser_id(browser);
            (task >= 0).then_some((task, pid))
        })
        .collect()
}
/// Main-thread owned sampler. Its runtime lease prevents CEF shutdown before
/// the native observer is removed; clients must release it when quitting.
pub struct TaskMonitor {
    manager: TaskManager,
    runtime: std::rc::Rc<crate::Runtime>,
}
pub(crate) fn monitor(runtime: std::rc::Rc<crate::Runtime>) -> Result<TaskMonitor, String> {
    let manager = task_manager_get().ok_or("Task manager is unavailable on this thread.")?;
    Ok(TaskMonitor { manager, runtime })
}
impl TaskMonitor {
    pub fn snapshot(&self) -> Result<Vec<TaskRecord>, String> {
        crate::shutdown::check_thread()?;
        self.runtime.lifecycle.require_running()?;
        let manager = &self.manager;
        let count = manager.tasks_count();
        if count > 16384 {
            return Err("Task inventory exceeds its limit.".into());
        }
        let mut ids = vec![0; count];
        if manager.task_ids_list(Some(&mut ids)) == 0 {
            return Err("Cannot read task inventory.".into());
        }
        let raw = manager.get_raw();
        let mut rows = Vec::with_capacity(ids.len());
        let processes = process_tasks(manager);
        for id in ids {
            // cef-rs 154's TaskManager::task_info currently copies into a temporary
            // without copying the output back. Use the public C ABI at this boundary.
            let mut info: sys::cef_task_info_t = unsafe { std::mem::zeroed() };
            info.size = std::mem::size_of::<sys::cef_task_info_t>();
            let success = unsafe {
                (*raw).get_task_info.ok_or("Missing task-info method.")?(raw, id, &mut info)
            };
            if success == 0 {
                continue;
            }
            let title = CefString::from(&info.title as *const sys::cef_string_utf16_t).to_string();
            rows.push(TaskRecord {
                id: info.id,
                title,
                kind: kind(info.type_),
                killable: info.is_killable != 0,
                // Renderer identities arrive over the scoped CEF helper channel,
                // then CEF maps the browser ID to its current task. Other tasks
                // need an extended CEF task-info API; task IDs are never PIDs.
                process_id: if kind(info.type_) == TaskKind::Browser {
                    Some(std::process::id())
                } else {
                    processes.get(&id).copied()
                },
                network_bytes_per_second: None,
                memory_bytes: (info.memory >= 0).then_some(info.memory as u64),
                cpu_percent: (info.cpu_usage.is_finite() && info.cpu_usage >= 0.)
                    .then_some(info.cpu_usage),
            });
            unsafe {
                sys::cef_string_utf16_clear(&mut info.title);
            }
        }
        Ok(rows)
    }
    pub fn end(&self, id: i64) -> Result<(), String> {
        // Resolve again at execution, rather than treating a displayed row as a
        // retained CEF capability. Task IDs are unique for the application lifetime.
        if !self
            .snapshot()?
            .iter()
            .any(|row| row.id == id && row.killable)
        {
            return Err("The task is no longer available for termination.".into());
        }
        if self.manager.kill_task(id) == 0 {
            return Err("Chromium could not terminate the task.".into());
        }
        Ok(())
    }
}

impl TaskSampler for TaskMonitor {
    fn snapshot(&self) -> Result<Vec<TaskRecord>, String> {
        TaskMonitor::snapshot(self)
    }
    fn end(&self, id: i64) -> Result<(), String> {
        TaskMonitor::end(self, id)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn row(id: i64, process_id: Option<u32>) -> TaskRecord {
        TaskRecord {
            id,
            title: format!("Task {id}"),
            kind: TaskKind::Renderer,
            killable: true,
            memory_bytes: Some(1024),
            cpu_percent: Some(0.),
            process_id,
            network_bytes_per_second: None,
        }
    }
    #[test]
    fn grouping_uses_only_real_process_identity_and_preserves_task_order() {
        let groups = group_tasks(vec![
            row(1, Some(200)),
            row(2, None),
            row(3, Some(300)),
            row(4, Some(200)),
            row(5, None),
            row(6, Some(0)),
        ]);
        assert_eq!(
            groups
                .iter()
                .map(|g| g.iter().map(|r| r.id).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            vec![vec![1, 4], vec![2], vec![3], vec![5], vec![6]]
        );
    }
    #[test]
    fn cef_gpu_utility_and_extension_kinds_are_not_collapsed_into_other() {
        use sys::cef_task_type_t::*;
        for (input, expected) in [
            (CEF_TASK_TYPE_GPU, TaskKind::Gpu),
            (CEF_TASK_TYPE_UTILITY, TaskKind::Utility),
            (CEF_TASK_TYPE_EXTENSION, TaskKind::Extension),
            (CEF_TASK_TYPE_GUEST, TaskKind::Renderer),
            (CEF_TASK_TYPE_SERVICE_WORKER, TaskKind::Worker),
        ] {
            assert_eq!(kind(input), expected);
        }
    }
    #[test]
    fn renderer_identity_is_bounded_and_expires_with_its_document_or_browser() {
        for text in ["0", "-1", " 42", "42x", "4294967296", "99999999999", ""] {
            assert_eq!(decode_pid(text), None);
        }
        assert_eq!(decode_pid("4294967295"), Some(u32::MAX));
        let mut identities = ProcessIdentities::default();
        identities.record(1, 2, 100);
        identities.record(2, 4, 200);
        identities.retain_current(|browser| if browser == 1 { Some(3) } else { Some(4) });
        assert!(!identities.0.contains_key(&1));
        assert_eq!(identities.0.remove(&2), Some((4, 200)));
        assert!(identities.0.is_empty());
    }
}
