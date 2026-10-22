//! Windows test data only. Actual access-policy verification stays in oe-core.
use std::{path::Path, process::Command};

pub fn private(path: &Path) {
    policy(path, None);
}

pub fn grant_other(path: &Path, write: bool) {
    policy(path, Some(if write { "Write" } else { "Read" }));
}

fn policy(path: &Path, other: Option<&str>) {
    // Match existing Windows release-test setup. Only synthetic fixtures are
    // changed; the host never invokes a shell or repairs a user's permissions.
    let text = path.to_string_lossy();
    let text = text
        .strip_prefix(r"\\?\")
        .unwrap_or(&text)
        .replace('\'', "''");
    let extra = other.map(|rights| format!(
        "$other=[System.Security.Principal.SecurityIdentifier]::new('S-1-1-0'); \
         $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($other,'{rights}','Allow'));"
    )).unwrap_or_default();
    let script = format!(
        "$ErrorActionPreference='Stop'; \
         $user=[System.Security.Principal.WindowsIdentity]::GetCurrent().User; \
         $acl=[System.Security.AccessControl.FileSecurity]::new(); \
         $acl.SetOwner($user); $acl.SetAccessRuleProtection($true,$false); \
         $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($user,'FullControl','Allow')); \
         {extra} Set-Acl -LiteralPath '{text}' -AclObject $acl;"
    );
    let result = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "synthetic ACL setup failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}
