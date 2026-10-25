//! Owned task data. CEF handles stay on its UI thread and never cross the facade.
use cef::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskKind {
    Browser,
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
}
fn kind(value: sys::cef_task_type_t) -> TaskKind {
    use sys::cef_task_type_t::*;
    match value {
        CEF_TASK_TYPE_BROWSER => TaskKind::Browser,
        CEF_TASK_TYPE_RENDERER | CEF_TASK_TYPE_GUEST => TaskKind::Renderer,
        CEF_TASK_TYPE_EXTENSION => TaskKind::Extension,
        CEF_TASK_TYPE_DEDICATED_WORKER
        | CEF_TASK_TYPE_SHARED_WORKER
        | CEF_TASK_TYPE_SERVICE_WORKER => TaskKind::Worker,
        _ => TaskKind::Other,
    }
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
