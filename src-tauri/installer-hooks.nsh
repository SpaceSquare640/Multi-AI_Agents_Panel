; Self-healing for the "Error opening file for writing:
; python-windows\_bz2.pyd" install failure: the app spawns bundled
; python.exe subprocesses (Skills/ML Engine bridges) at every launch,
; and on a build predating the app's own RunEvent::Exit cleanup, closing
; the window could leave those python.exe children running as orphans,
; still holding the installed python-windows/*.pyd files open. The next
; install then can't overwrite them. Force-clear both before extracting
; any files, so an update always succeeds regardless of which build the
; user is currently running.
;
; Backtick-delimited NSIS strings throughout (rather than the more usual
; single/double quotes) specifically so the embedded PowerShell/taskkill
; quoting below needs no NSIS-level escaping at all — only the ordinary
; escaping those commands would need on any plain command line.
!macro NSIS_HOOK_PREINSTALL
  ; The main app window, if still open for any reason. Errors ignored
  ; (Pop discards the exit code) — "nothing to kill" is the common case.
  nsExec::ExecToLog `taskkill /F /IM "Multi-AI Agents Panel.exe" /T`
  Pop $0

  ; Only python.exe instances actually running out of this app's own
  ; bundled python-windows folder — never a bare "taskkill /IM python.exe",
  ; which would kill the user's unrelated Python processes too.
  nsExec::ExecToLog `powershell -NoProfile -Command "Get-CimInstance Win32_Process -Filter \"Name='python.exe'\" -ErrorAction SilentlyContinue | Where-Object { $_.ExecutablePath -like '*python-windows*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }"`
  Pop $0
!macroend
