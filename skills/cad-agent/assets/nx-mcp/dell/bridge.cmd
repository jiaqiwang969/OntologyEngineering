@echo off
rem NX MCP batch bridge: headless NX session (run_journal) that serves NX_MCP bridge requests.
rem Started detached by start-bridge.ps1 via WMI so it survives the SSH session that launched it.
set "NX_MCP_WORKSPACE=%USERPROFILE%\work"
set "NX_MCP_BRIDGE_STOP_FILE=%USERPROFILE%\work\nxmcp\stop.flag"
set NX_MCP_ALLOW_UNVERIFIED_PYTHON_BRIDGE=1
set NX_MCP_ENABLE_EXPERIMENTAL=1
set NX_MCP_ENABLE_JOURNAL=1
set PYTHONUTF8=1
cd /d "%USERPROFILE%\work\nxmcp"
"C:\Program Files\Siemens\NX 2412\NXBIN\run_journal.exe" "%~1" >> "%USERPROFILE%\work\nxmcp\bridge.log" 2>&1
