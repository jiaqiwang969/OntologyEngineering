//! New child ownership only. Never binds a pre-existing CAD instance or changes
//! the input desktop. Failed attachment leaves the exact child identity recorded.
use crate::{
    com::{self, Apartment, Trace},
    installation::{Api, observe_file},
};
use anyhow::{Context, Result, ensure};
use oe_contracts::{SolidWorksNativeFilePin, SolidWorksNativeProcess};
use oe_windows_controller::windows::{ProcessObservation, ProcessWatch};
use serde_json::json;
use std::{
    os::windows::io::{FromRawHandle, OwnedHandle},
    path::Path,
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        System::{
            Com::*, Diagnostics::ToolHelp::*, RemoteDesktop::ProcessIdToSessionId,
            StationsAndDesktops::*, Threading::*,
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    },
    core::{Interface, PCWSTR, PWSTR},
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub fn process_record(watch: &ProcessWatch) -> Result<SolidWorksNativeProcess> {
    let id = watch.identity();
    let mut session = 0;
    unsafe {
        ProcessIdToSessionId(id.pid, &mut session)?;
    }
    let image = observe_file(&watch.image_path()?, 64 * 1024 * 1024)?;
    Ok(SolidWorksNativeProcess {
        pid: u64::from(id.pid),
        creation_filetime: id.creation_time_100ns,
        session_id: u64::from(session),
        image,
    })
}
pub fn cad_processes() -> Result<Vec<u32>> {
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)? };
    let _guard = unsafe { OwnedHandle::from_raw_handle(raw.0) };
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut out = Vec::new();
    let mut count = 0;
    let mut result = unsafe { Process32FirstW(raw, &mut entry) };
    loop {
        if let Err(error) = result {
            ensure!(
                error.code() == windows::core::HRESULT::from_win32(ERROR_NO_MORE_FILES.0),
                "solidworks_process_inventory_failed:{error}"
            );
            break;
        }
        count += 1;
        ensure!(count <= 65536, "solidworks_process_inventory_budget");
        let end = entry
            .szExeFile
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(entry.szExeFile.len());
        if String::from_utf16_lossy(&entry.szExeFile[..end]).eq_ignore_ascii_case("sldworks.exe") {
            out.push(entry.th32ProcessID);
        }
        result = unsafe { Process32NextW(raw, &mut entry) };
    }
    out.sort_unstable();
    Ok(out)
}
struct Desktop(HDESK);
impl Drop for Desktop {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseDesktop(self.0);
        }
    }
}
fn desktop_name(handle: HDESK) -> Result<String> {
    let mut text = vec![0u16; 512];
    let mut needed = 0;
    unsafe {
        GetUserObjectInformationW(
            HANDLE(handle.0),
            UOI_NAME,
            Some(text.as_mut_ptr().cast()),
            (text.len() * 2) as u32,
            Some(&mut needed),
        )?;
    }
    ensure!(
        needed > 0 && needed as usize <= text.len() * 2,
        "solidworks_desktop_name_bound"
    );
    let end = text
        .iter()
        .position(|v| *v == 0)
        .context("solidworks_desktop_name_terminator")?;
    Ok(String::from_utf16(&text[..end])?)
}
fn input_desktop_name() -> Result<String> {
    let desktop =
        Desktop(unsafe { OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS)? });
    desktop_name(desktop.0)
}
fn process_windows_on_desktop(desktop: HDESK, pid: u32) -> Result<Vec<usize>> {
    struct Search {
        pid: u32,
        windows: Vec<usize>,
    }
    unsafe extern "system" fn collect(window: HWND, param: LPARAM) -> BOOL {
        let search = unsafe { &mut *(param.0 as *mut Search) };
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(window, Some(&mut pid));
        }
        if pid == search.pid {
            search.windows.push(window.0 as usize);
        }
        BOOL(1)
    }
    let mut search = Search {
        pid,
        windows: Vec::new(),
    };
    unsafe {
        EnumDesktopWindows(
            desktop,
            Some(collect),
            LPARAM((&mut search as *mut Search) as isize),
        )?;
    }
    Ok(search.windows)
}
fn read_only_process(pid: u32) -> Result<ProcessWatch> {
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            false,
            pid,
        )?
    };
    let handle = unsafe { OwnedHandle::from_raw_handle(handle.0) };
    Ok(ProcessWatch::from_owned_handle(&handle)?)
}
fn foreground_record() -> serde_json::Value {
    let window = unsafe { GetForegroundWindow() };
    let mut pid = 0;
    let tid = unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
    json!({"hwnd":window.0 as usize,"pid":pid,"thread_id":tid})
}
enum ForegroundCommand {
    Read(mpsc::Sender<serde_json::Value>),
    Stop,
}
struct InputForeground {
    sender: mpsc::Sender<ForegroundCommand>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl InputForeground {
    fn start(expected_desktop: &str) -> Result<Self> {
        let (sender, receive) = mpsc::channel();
        let (ready, ready_receive) = mpsc::channel();
        let expected = expected_desktop.to_owned();
        let thread = std::thread::spawn(move || {
            let actual = (|| -> Result<String> {
                desktop_name(unsafe { GetThreadDesktop(GetCurrentThreadId())? })
            })();
            let valid = actual.as_ref().is_ok_and(|name| *name == expected);
            let _ = ready.send(actual.map_err(|error| format!("{error:#}")));
            if !valid {
                return;
            }
            while let Ok(command) = receive.recv() {
                match command {
                    ForegroundCommand::Read(reply) => {
                        let _ = reply.send(foreground_record());
                    }
                    ForegroundCommand::Stop => return,
                }
            }
        });
        let result = Self {
            sender,
            thread: Some(thread),
        };
        let desktop = ready_receive
            .recv_timeout(Duration::from_secs(5))?
            .map_err(anyhow::Error::msg)?;
        ensure!(
            desktop == expected_desktop,
            "solidworks_input_observer_desktop_mismatch"
        );
        Ok(result)
    }
    fn read(&self) -> Result<serde_json::Value> {
        let (send, receive) = mpsc::channel();
        self.sender.send(ForegroundCommand::Read(send))?;
        Ok(receive.recv_timeout(Duration::from_secs(2))?)
    }
}
impl Drop for InputForeground {
    fn drop(&mut self) {
        let _ = self.sender.send(ForegroundCommand::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
/// Enter before the worker's first CoInitialize. Only our thread association
/// changes; the input desktop is never switched.
pub struct PrivateDesktop {
    desktop: Desktop,
    name: String,
    input_name: String,
    original: HDESK,
    foreground: InputForeground,
}
impl PrivateDesktop {
    pub fn enter(job: &str, trace: &mut Trace) -> Result<Rc<Self>> {
        let worker = process_record(&ProcessWatch::current()?)?;
        ensure!(
            worker.session_id != 0,
            "solidworks_noninteractive_session_cannot_prove_desktop_execution"
        );
        let input_name = input_desktop_name()?;
        let original = unsafe { GetThreadDesktop(GetCurrentThreadId())? };
        ensure!(
            desktop_name(original)? == input_name,
            "solidworks_worker_initial_desktop_not_input"
        );
        let foreground = InputForeground::start(&input_name)?;
        let nonce = oe_windows_controller::windows::random_nonce()?;
        let nonce: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
        let name = format!("oev2-sw-{job}-{nonce}");
        let encoded = wide(&name);
        let desktop = Desktop(unsafe {
            CreateDesktopW(
                PCWSTR(encoded.as_ptr()),
                PCWSTR::null(),
                None,
                DESKTOP_CONTROL_FLAGS(0),
                0x000f01ff,
                None,
            )?
        });
        ensure!(
            desktop_name(desktop.0)? == name && name != input_name,
            "solidworks_private_desktop_identity_mismatch"
        );
        trace.push(json!({"phase":"worker_desktop_association_intent","worker":worker,"private_desktop":name,"input_desktop":input_name,"input_foreground":foreground.read()?,"com_initialized":false,"input_desktop_switch":"never"}))?;
        unsafe {
            SetThreadDesktop(desktop.0)?;
        }
        let result = Rc::new(Self {
            desktop,
            name,
            input_name,
            original,
            foreground,
        });
        ensure!(
            desktop_name(unsafe { GetThreadDesktop(GetCurrentThreadId())? })? == result.name
                && input_desktop_name()? == result.input_name,
            "solidworks_worker_private_desktop_association_mismatch"
        );
        trace.push(json!({"phase":"worker_desktop_associated_before_com","worker":worker,"worker_thread":unsafe{GetCurrentThreadId()},"private_desktop":result.name,"input_desktop":result.input_name,"input_foreground":result.foreground.read()?,"com_initialized":false,"input_desktop_switch":"never"}))?;
        Ok(result)
    }
}
impl Drop for PrivateDesktop {
    fn drop(&mut self) {
        // Native drops owned objects and STA before this context. Restore only
        // our thread association; never switch the displayed/input desktop.
        unsafe {
            let _ = SetThreadDesktop(self.original);
        }
    }
}
fn owned_orphan_desktop(pid: u32) -> Result<bool> {
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)? };
    let _snapshot = unsafe { OwnedHandle::from_raw_handle(raw.0) };
    let mut thread = THREADENTRY32 {
        dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let mut next = unsafe { Thread32First(raw, &mut thread) };
    let mut count = 0;
    loop {
        if let Err(error) = next {
            ensure!(
                error.code() == windows::core::HRESULT::from_win32(ERROR_NO_MORE_FILES.0),
                "solidworks_thread_inventory_failed:{error}"
            );
            return Ok(false);
        }
        count += 1;
        ensure!(count <= 262144, "solidworks_thread_inventory_budget");
        if thread.th32OwnerProcessID == pid {
            if let Ok(desktop) = unsafe { GetThreadDesktop(thread.th32ThreadID) } {
                if desktop_name(desktop)?.starts_with("oev2-sw-") {
                    return Ok(true);
                }
            }
        }
        next = unsafe { Thread32Next(raw, &mut thread) };
    }
}
pub struct OwnedCad {
    pub app: Option<IDispatch>,
    pub watch: ProcessWatch,
    pub identity: SolidWorksNativeProcess,
    _desktop: Rc<PrivateDesktop>,
    _writer: OwnedHandle,
    desktop_name: String,
    input_name: String,
    main_thread_id: u32,
    foreign: Vec<ProcessWatch>,
}
impl OwnedCad {
    pub fn spawn(
        executable: &SolidWorksNativeFilePin,
        profile: Option<&str>,
        desktop: Option<Rc<PrivateDesktop>>,
        directory: &Path,
        apartment: &Apartment,
        api: &Api,
        trace: &mut Trace,
    ) -> Result<Self> {
        let existing = cad_processes()?;
        trace.push(json!({"phase":"pre_spawn_inventory","existing_cad_pids":existing,"execution_profile":profile,"attachment":"never"}))?;
        if profile.is_none() {
            ensure!(
                existing.is_empty(),
                "solidworks_existing_instance_ownership_unavailable"
            );
        } else {
            ensure!(
                profile == Some("owned_private_desktop"),
                "solidworks_unknown_execution_profile"
            );
        }
        let worker = process_record(&ProcessWatch::current()?)?;
        ensure!(
            worker.session_id != 0,
            "solidworks_noninteractive_session_cannot_prove_desktop_execution"
        );
        // Process-wide named resource admission; spoofed/inaccessible names fail closed.
        // A previous orphan is separately refused after the worker handle was released.
        let mutex_name = wide(&format!("Global\\oev2-sw-writer-{}", executable.sha256));
        let handle = unsafe { CreateMutexW(None, true, PCWSTR(mutex_name.as_ptr()))? };
        let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let writer = unsafe { OwnedHandle::from_raw_handle(handle.0) };
        ensure!(!already_exists, "solidworks_owned_writer_busy");
        let mut foreign = Vec::new();
        let mut foreign_records = Vec::new();
        for pid in &existing {
            ensure!(
                !owned_orphan_desktop(*pid)?,
                "solidworks_prior_owned_child_requires_reconciliation"
            );
            let watch = read_only_process(*pid)?;
            foreign_records.push(process_record(&watch)?);
            foreign.push(watch);
        }
        let desktop = desktop.context("solidworks_worker_private_desktop_context_missing")?;
        let name = desktop.name.clone();
        let input_name = desktop.input_name.clone();
        ensure!(
            desktop_name(unsafe { GetThreadDesktop(GetCurrentThreadId())? })? == name,
            "solidworks_worker_com_desktop_mismatch"
        );
        trace.push(json!({"phase":"foreign_processes_observed","processes":foreign_records,"control":"none","input_desktop":input_name,"foreground":desktop.foreground.read()?,"writer_mutex":"held"}))?;
        let exe = wide(&executable.path);
        let mut command = wide(&format!("\"{}\"", executable.path));
        let cwd = wide(
            directory
                .to_str()
                .context("solidworks_output_path_unicode")?,
        );
        let mut desktop_w = wide(&name);
        let startup = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            lpDesktop: PWSTR(desktop_w.as_mut_ptr()),
            ..Default::default()
        };
        let mut info = PROCESS_INFORMATION::default();
        trace.push(json!({"phase":"before_create_process","executable":executable,"private_desktop":name,"input_desktop_switch":"never","outcome":"unknown_until_return"}))?;
        unsafe {
            CreateProcessW(
                PCWSTR(exe.as_ptr()),
                PWSTR(command.as_mut_ptr()),
                None,
                None,
                false,
                PROCESS_CREATION_FLAGS(0),
                None,
                PCWSTR(cwd.as_ptr()),
                &startup,
                &mut info,
            )?;
        }
        let child = unsafe { OwnedHandle::from_raw_handle(info.hProcess.0) };
        let _thread = unsafe { OwnedHandle::from_raw_handle(info.hThread.0) };
        let watch = ProcessWatch::from_owned_handle(&child)?;
        let identity = process_record(&watch)?;
        trace.push(json!({"phase":"child_created","process":identity,"scope":"exact_direct_child","descendant_cleanup":"not_claimed"}))?;
        ensure!(
            identity.pid == u64::from(info.dwProcessId)
                && identity.image.sha256 == executable.sha256
                && identity.image.bytes == executable.bytes
                && identity.session_id == worker.session_id,
            "solidworks_child_identity_mismatch"
        );
        let mut owned = Self {
            app: None,
            watch,
            identity,
            _desktop: desktop,
            _writer: writer,
            desktop_name: name.clone(),
            input_name,
            main_thread_id: info.dwThreadId,
            foreign,
        };
        // Only the moniker naming our retained process can be obtained. No
        // GetActiveObject / CoCreateInstance fallback or enumeration of objects.
        let name = format!("SolidWorks_PID_{}", owned.identity.pid);
        let start = Instant::now();
        let mut desktop_pending_reported = false;
        while start.elapsed() < Duration::from_secs(90) {
            if let ProcessObservation::Exited { exit_code } = owned.watch.poll()? {
                trace.push(json!({"phase":"child_exited_before_binding","process":owned.identity,"exit_code":exit_code,"elapsed_ms":start.elapsed().as_millis(),"cad_binding":"not_established"}))?;
                anyhow::bail!("solidworks_child_exited_before_binding:{exit_code}");
            }
            // A newly created GUI thread may not yet expose its desktop handle.
            // Readiness requires the actual retained child, exact primary thread
            // returned by CreateProcess, and positive desktop/window observations.
            // Elapsed time or a guessed alternative thread never proves readiness.
            if let Err(error) = unsafe { GetThreadDesktop(owned.main_thread_id) } {
                if !desktop_pending_reported {
                    trace.push(json!({"phase":"primary_thread_desktop_pending","process":owned.identity,"main_thread_id":owned.main_thread_id,"hresult":format!("0x{:08x}",error.code().0 as u32),"readiness":"unknown; no COM attachment","deadline_seconds":90}))?;
                    desktop_pending_reported = true;
                }
                apartment.pump();
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            owned.verify_isolation(trace)?;
            if let Some(app) = owned_moniker(&name)? {
                let (pid, _) = trace.invoke(&app, api, "ISldWorks.GetProcessID", vec![])?;
                ensure!(
                    com::integer(&pid)? as u64 == owned.identity.pid,
                    "solidworks_com_process_mismatch"
                );
                owned.app = Some(app);
                break;
            }
            apartment.pump();
            std::thread::sleep(Duration::from_millis(200));
        }
        ensure!(
            owned.app.is_some(),
            "solidworks_owned_rot_binding_timeout_child_left_recorded"
        );
        let app = owned.app.as_ref().unwrap();
        let (count, _) = trace.invoke(app, api, "ISldWorks.GetDocumentCount", vec![])?;
        ensure!(
            com::integer(&count)? == 0,
            "solidworks_new_child_has_unowned_documents"
        );
        trace.push(json!({"phase":"owned_application_bound","process":owned.identity,"moniker":name,"document_count":0}))?;
        Ok(owned)
    }
    pub fn verify_isolation(&self, trace: &mut Trace) -> Result<()> {
        let child_desktop = desktop_name(unsafe { GetThreadDesktop(self.main_thread_id)? })?;
        let input_name = input_desktop_name()?;
        ensure!(
            child_desktop == self.desktop_name
                && input_name == self.input_name
                && child_desktop != input_name,
            "solidworks_desktop_isolation_changed"
        );
        let foreground = self._desktop.foreground.read()?;
        ensure!(
            foreground["pid"] != self.identity.pid,
            "solidworks_child_became_foreground"
        );
        let input = Desktop(unsafe {
            OpenInputDesktop(
                DESKTOP_CONTROL_FLAGS(0),
                false,
                DESKTOP_ACCESS_FLAGS(DESKTOP_READOBJECTS.0 | DESKTOP_ENUMERATE.0),
            )?
        });
        let input_windows = process_windows_on_desktop(input.0, self.identity.pid as u32)?;
        ensure!(
            input_windows.is_empty(),
            "solidworks_child_windows_on_input_desktop"
        );
        let private_windows =
            process_windows_on_desktop(self._desktop.desktop.0, self.identity.pid as u32)?;
        let mut expected: Vec<u32> = self.foreign.iter().map(|w| w.identity().pid).collect();
        let mut records = Vec::new();
        for watch in &self.foreign {
            ensure!(
                matches!(watch.poll()?, ProcessObservation::Running),
                "solidworks_foreign_process_state_changed"
            );
            records.push(process_record(watch)?);
        }
        expected.push(self.identity.pid as u32);
        expected.sort_unstable();
        ensure!(
            cad_processes()? == expected,
            "solidworks_unexpected_concurrent_instance"
        );
        trace.push(json!({"phase":"isolation_observed","cad":self.identity,"main_thread_id":self.main_thread_id,"child_desktop":child_desktop,"input_desktop":input_name,"foreground":foreground,"child_windows_on_private_desktop":private_windows,"child_windows_on_input_desktop":input_windows,"foreign":records,"foreign_control":"none","desktop_switch":"never"}))?;
        Ok(())
    }
    pub fn exit(&mut self, apartment: &Apartment, api: &Api, trace: &mut Trace) -> Result<bool> {
        let app = self
            .app
            .take()
            .context("solidworks_exit_without_owned_binding")?;
        self.verify_isolation(trace)?;
        ensure!(
            matches!(self.watch.poll()?, ProcessObservation::Running),
            "solidworks_process_not_running_before_exit"
        );
        let (pid, _) = trace.invoke(&app, api, "ISldWorks.GetProcessID", vec![])?;
        ensure!(
            com::integer(&pid)? as u64 == self.identity.pid,
            "solidworks_exit_identity_changed"
        );
        trace.invoke(&app, api, "ISldWorks.ExitApp", vec![])?;
        drop(app);
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(30) {
            if let ProcessObservation::Exited { exit_code } = self.watch.poll()? {
                trace.push(json!({"phase":"cad_process_exit","process":self.identity,"exit_code":exit_code,"scope":"exact_direct_child"}))?;
                return Ok(true);
            }
            apartment.pump();
            std::thread::sleep(Duration::from_millis(100));
        }
        trace.push(json!({"phase":"cad_process_exit","process":self.identity,"status":"unknown_after_deadline","termination":"not_attempted"}))?;
        Ok(false)
    }
}
fn owned_moniker(expected: &str) -> Result<Option<IDispatch>> {
    let rot = unsafe { GetRunningObjectTable(0)? };
    let en = unsafe { rot.EnumRunning()? };
    let ctx = unsafe { CreateBindCtx(0)? };
    for _ in 0..4096 {
        let mut one = [None];
        let mut fetched = 0;
        let result = unsafe { en.Next(&mut one, Some(&mut fetched)) };
        result.ok()?;
        if fetched == 0 {
            return Ok(None);
        }
        let moniker = one[0].take().context("solidworks_rot_null_moniker")?;
        let ptr = unsafe { moniker.GetDisplayName(&ctx, None)? };
        let text = unsafe { ptr.to_string() };
        unsafe {
            CoTaskMemFree(Some(ptr.0.cast()));
        }
        let text = text?;
        if text == expected {
            return Ok(Some(unsafe { rot.GetObject(&moniker)? }.cast()?));
        }
    }
    anyhow::bail!("solidworks_rot_inventory_budget")
}
