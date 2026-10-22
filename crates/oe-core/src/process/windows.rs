//! Windows process-tree transport for the bounded external-process runner.
//!
//! The child is created suspended so there is no window in which it can
//! escape the job before assignment.  The job is configured to terminate all
//! assigned processes when its last handle closes; timeout and setup failure
//! paths explicitly terminate it as well.

use std::io;
use std::mem::{size_of, zeroed};
use std::os::windows::io::AsRawHandle;
use std::process::{Child, Command};

use windows_sys::Win32::Foundation::{BOOL, CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Threading::PROCESS_CREATION_FLAGS;
use windows_sys::Win32::System::Threading::{
    CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
};

fn win32_error() -> io::Error {
    io::Error::last_os_error()
}

fn failed(value: BOOL) -> bool {
    value == 0
}

/// Apply the creation flag before `Command::spawn` creates the child.
pub(crate) fn prepare_command(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(CREATE_SUSPENDED as PROCESS_CREATION_FLAGS);
}

pub(crate) struct Job {
    handle: HANDLE,
}

impl Job {
    pub(crate) fn new() -> io::Result<Self> {
        // A null security descriptor gives the child-process runner a private,
        // unnamed job without changing the caller's access-control policy.
        let handle =
            unsafe { CreateJobObjectW(std::ptr::null::<SECURITY_ATTRIBUTES>(), std::ptr::null()) };
        if handle == 0 {
            return Err(win32_error());
        }

        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if failed(configured) {
            unsafe { CloseHandle(handle) };
            return Err(win32_error());
        }
        Ok(Self { handle })
    }

    pub(crate) fn assign(&self, child: &Child) -> io::Result<()> {
        let process = child.as_raw_handle() as HANDLE;
        let assigned = unsafe { AssignProcessToJobObject(self.handle, process) };
        if failed(assigned) {
            Err(win32_error())
        } else {
            Ok(())
        }
    }

    /// Resume the primary thread after the process has been assigned to this
    /// job. `CREATE_SUSPENDED` guarantees that the first process thread is the
    /// one that must be resumed before user code can run.
    pub(crate) fn resume_primary(&self, process_id: u32) -> io::Result<()> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(win32_error());
        }

        let mut entry: THREADENTRY32 = unsafe { zeroed() };
        entry.dwSize = size_of::<THREADENTRY32>() as u32;
        let mut thread_id = None;
        let first = unsafe { Thread32First(snapshot, &mut entry) };
        if !failed(first) {
            loop {
                if entry.th32OwnerProcessID == process_id {
                    thread_id = Some(entry.th32ThreadID);
                    break;
                }
                let next = unsafe { Thread32Next(snapshot, &mut entry) };
                if failed(next) {
                    break;
                }
            }
        }
        unsafe { CloseHandle(snapshot) };

        let thread_id = thread_id.ok_or_else(win32_error)?;
        let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, thread_id) };
        if thread == 0 {
            return Err(win32_error());
        }
        let previous_suspend_count = unsafe { ResumeThread(thread) };
        unsafe { CloseHandle(thread) };
        if previous_suspend_count == u32::MAX {
            Err(win32_error())
        } else if previous_suspend_count == 0 {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "windows_primary_thread_was_not_suspended",
            ))
        } else {
            Ok(())
        }
    }

    pub(crate) fn terminate(&self) {
        // There is no useful recovery action if termination itself fails; the
        // child handle is still killed by the caller and Job::drop closes the
        // kill-on-close job handle.
        unsafe {
            let _ = TerminateJobObject(self.handle, 1);
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}
