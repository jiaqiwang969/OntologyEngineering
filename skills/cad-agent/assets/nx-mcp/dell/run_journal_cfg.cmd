@echo off
set "INSPECT_CFG=%USERPROFILE%\work\journals\%1.cfg.json"
if not defined NX_INSPECT_OUTPUT_ROOT set "NX_INSPECT_OUTPUT_ROOT=%USERPROFILE%\work\journals\out"
if not exist "%NX_INSPECT_OUTPUT_ROOT%" mkdir "%NX_INSPECT_OUTPUT_ROOT%"
"C:\Program Files\Siemens\NX 2412\NXBIN\run_journal.exe" "%USERPROFILE%\work\journals\%2.py" > "%NX_INSPECT_OUTPUT_ROOT%\%1.stdout.txt" 2>&1
echo rc=%ERRORLEVEL%
