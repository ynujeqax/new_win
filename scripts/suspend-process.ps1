# Suspends (or with -Resume, resumes) every thread of a process, to test a hung NewWin shell.
param([Parameter(Mandatory)][int]$ProcessId, [switch]$Resume)

Add-Type -Namespace Native -Name Nt -MemberDefinition @'
[DllImport("ntdll.dll")] public static extern int NtSuspendProcess(IntPtr h);
[DllImport("ntdll.dll")] public static extern int NtResumeProcess(IntPtr h);
[DllImport("kernel32.dll")] public static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
[DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h);
'@

$PROCESS_SUSPEND_RESUME = 0x0800
$h = [Native.Nt]::OpenProcess($PROCESS_SUSPEND_RESUME, $false, $ProcessId)
if ($h -eq [IntPtr]::Zero) { throw "cannot open process $ProcessId" }
$status = if ($Resume) { [Native.Nt]::NtResumeProcess($h) } else { [Native.Nt]::NtSuspendProcess($h) }
[void][Native.Nt]::CloseHandle($h)
if ($status -ne 0) { throw ("NtSuspend/ResumeProcess failed: 0x{0:X}" -f $status) }
Write-Output ("{0} pid {1}" -f $(if ($Resume) { "resumed" } else { "suspended" }), $ProcessId)
