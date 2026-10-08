<#
Builds LED Badge Studio on Windows 10 or 11, installing anything that's missing.
Easiest: double-click build.cmd in the project folder (or run
"build.cmd --install"). Or, in PowerShell:

    powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1 [-Install] [-Arch x64|arm64]

-Arch picks the processor to build for: x64 (also x86_64, amd64) or arm64
(also aarch64). Without it, the one this Windows runs on.

What it does:
  1. Installs Visual Studio 2022 Build Tools (C++ compiler and Windows SDK)
     with winget if they're missing, with the C++ tools for the processor
     built for (adding them to an existing install with the Visual Studio
     Installer). Windows asks for permission, and this part takes a while
     (several GB).
  2. Installs Rust (rustup) with winget if it's missing.
  3. Finds the Windows SDK's shader compiler (fxc.exe), which GPUI needs.
  4. Applies branding.toml (app name, copyright, icons; see
     docs\branding.md), builds led-badge-studio.exe and badgectl.exe, and zips them with the README and
     licenses into dist\LED_Badge_Studio-windows-<arch>.zip (put together in target\package\ first,
     removed again at the end; dist\ only gets the download).
  5. With -Install: copies the app to %LOCALAPPDATA%\Programs\<app name>,
     adds a Start menu shortcut and makes .badge project files open with it
     (for the current user; no administrator rights needed).
#>
param([switch]$Install, [string]$Arch)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

# The processor Windows runs on. Not this process's view: an x64 PowerShell
# runs emulated on ARM (when started from another x64 program), and there
# PROCESSOR_ARCHITECTURE and .NET's OSArchitecture both say x64. The
# system-wide setting in the registry, and the processor itself, don't.
function Native-Arch {
    try {
        $system = Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment'
        if ($system.PROCESSOR_ARCHITECTURE -eq 'ARM64') { return 'arm64' }
        if ($system.PROCESSOR_ARCHITECTURE -eq 'AMD64') { return 'x64' }
    } catch {}
    try {
        # Win32_Processor.Architecture: 12 is ARM64, 9 is x64.
        $cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
        if ($cpu.Architecture -eq 12) { return 'arm64' }
        if ($cpu.Architecture -eq 9) { return 'x64' }
    } catch {}
    if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { return 'arm64' } else { return 'x64' }
}

$Arch = if ($Arch) { $Arch.ToLowerInvariant() } else { Native-Arch }
switch ($Arch) {
    { $_ -in 'x64', 'x86_64', 'amd64' } { $Arch = 'x64'; $rustTarget = 'x86_64-pc-windows-msvc'
        $msvcComponent = 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64' }
    { $_ -in 'arm64', 'aarch64' } { $Arch = 'arm64'; $rustTarget = 'aarch64-pc-windows-msvc'
        $msvcComponent = 'Microsoft.VisualStudio.Component.VC.Tools.ARM64' }
    default { throw "Unknown -Arch '$Arch': use x64 or arm64." }
}

function Step($text) { Write-Host "`n== $text" -ForegroundColor Cyan }

# Runs a program with its output hidden; returns its exit code. (Windows
# PowerShell turns a hidden program's messages on stderr, such as rustup's
# "info:" lines, into errors, which 'Stop' above makes fatal; not here.)
function Quietly([scriptblock]$command) {
    $ErrorActionPreference = 'Continue'
    & $command *> $null
    return $LASTEXITCODE
}

function Need-Winget {
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        throw "winget isn't available. Install 'App Installer' from the Microsoft Store, then run this again."
    }
}

function Winget-Install([string]$id, [string[]]$extra) {
    Need-Winget
    # From winget's own source: with the Microsoft Store source also
    # searched, a failure there (a certificate error, for one) makes winget
    # stop and ask which source to use.
    $wingetArgs = @('install', '--id', $id, '-e', '--source', 'winget',
        '--accept-package-agreements', '--accept-source-agreements') + $extra
    & winget @wingetArgs
    # winget's exit code isn't reliable (e.g. "already installed"); callers
    # check for the tool afterwards instead.
}

$vsInstaller = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer'

# An installation of Visual Studio (or its Build Tools) with `$component`;
# without one, any installation.
function Find-VisualStudio([string]$component) {
    $vswhere = Join-Path $vsInstaller 'vswhere.exe'
    if (-not (Test-Path $vswhere)) { return $null }
    $vsArgs = @('-latest', '-products', '*', '-property', 'installationPath')
    if ($component) { $vsArgs += @('-requires', $component) }
    $path = & $vswhere @vsArgs
    if ($path) { return $path } else { return $null }
}

# The C++ tools for the processor built for.
function Find-Msvc { Find-VisualStudio $msvcComponent }

function Find-Fxc {
    $bin = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    if (-not (Test-Path $bin)) { return $null }
    $versions = Get-ChildItem $bin -Directory | Where-Object { $_.Name -match '^\d+\.\d+\.\d+\.\d+$' } |
        Sort-Object { [version]$_.Name } -Descending
    foreach ($v in $versions) {
        # (Not $arch: PowerShell names ignore case, and $Arch is the target.)
        foreach ($binArch in @('x64', 'arm64', 'x86')) {
            $fxc = Join-Path $v.FullName "$binArch\fxc.exe"
            if (Test-Path $fxc) { return $fxc }
        }
    }
    return $null
}

function Add-CargoToPath {
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    if ((Test-Path $cargoBin) -and ($env:Path -notlike "*$cargoBin*")) { $env:Path = "$cargoBin;$env:Path" }
}

Step "Checking the C++ build tools ($Arch)"
if (-not (Find-Msvc)) {
    $existing = Find-VisualStudio ''
    if ($existing) {
        # Installed, without the tools for this processor: add them.
        Write-Host "Adding the C++ tools for $Arch to $existing (Windows asks for permission)..."
        $setup = Join-Path $vsInstaller 'setup.exe'
        $p = Start-Process -FilePath $setup -Wait -PassThru -Verb RunAs -ArgumentList @(
            'modify', '--installPath', "`"$existing`"", '--add', $msvcComponent, '--quiet', '--norestart')
        if ($p.ExitCode -notin 0, 3010) { Write-Warning "The Visual Studio Installer exited with $($p.ExitCode)." }
    } else {
        Write-Host "Installing Visual Studio 2022 Build Tools (C++ workload for $Arch, Windows SDK)..."
        Winget-Install 'Microsoft.VisualStudio.2022.BuildTools' @('--override',
            "--quiet --wait --norestart --nocache --add Microsoft.VisualStudio.Workload.VCTools --add $msvcComponent --includeRecommended")
    }
    if (-not (Find-Msvc)) { throw "The C++ build tools for $Arch are still missing. In the Visual Studio Installer, add the ""Desktop development with C++"" workload and the ""MSVC ... $Arch build tools"" component, then run this again." }
}
Write-Host "C++ build tools ($Arch): OK"

Step 'Checking the Windows SDK shader compiler (fxc.exe)'
$fxc = Find-Fxc
if (-not $fxc) {
    Write-Host 'Installing the Windows 11 SDK...'
    Winget-Install 'Microsoft.WindowsSDK.10.0.26100' @()
    $fxc = Find-Fxc
    if (-not $fxc) { throw 'fxc.exe was not found. Install the Windows 11 SDK, then run this again.' }
}
$env:GPUI_FXC_PATH = $fxc
Write-Host "fxc.exe: $fxc"

Step 'Checking Rust'
Add-CargoToPath
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host 'Installing Rust (rustup)...'
    Winget-Install 'Rustlang.Rustup' @()
    Add-CargoToPath
    if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) { throw 'Rust was not installed. Install it from https://rustup.rs, then run this again.' }
}
if ((Quietly { rustup show active-toolchain }) -ne 0) {
    & rustup toolchain install stable --profile minimal
    & rustup default stable
}
# The latest Rust.
Write-Host 'Updating Rust (rustup update stable)...'
if ((Quietly { rustup update stable --no-self-update }) -ne 0) { Write-Warning "Couldn't update Rust; using the version installed." }
if ((Quietly { rustup target add $rustTarget }) -ne 0) { throw "Couldn't add Rust's $rustTarget target." }
Write-Host "Rust: $(& rustc --version), building for $rustTarget"

Step 'Applying branding.toml (name, copyright, icons)'
$brandingTool = @('run', '--quiet', '--release', '--locked', '--manifest-path', (Join-Path $root 'packaging\branding\Cargo.toml'), '--')
& cargo @brandingTool
if ($LASTEXITCODE -ne 0) { throw 'Applying branding.toml failed (see the messages above).' }
$appName = (& cargo @brandingTool --print name | Out-String).Trim()

Step "Building $appName for $Arch (the first build takes several minutes)"
Push-Location $root
try {
    & cargo build --release --target $rustTarget -p badge-app -p badgectl
    if ($LASTEXITCODE -ne 0) { throw 'The build failed (see the messages above).' }
} finally { Pop-Location }

Step 'Packaging'
$name = "LED_Badge_Studio-windows-$Arch"
$dist = Join-Path $root 'dist'
# Put together here; only the zip goes to dist\.
$out = Join-Path $root "target\package\$name"
# Removed when the script ends, however it ends: it's only needed until the
# zip is made and -Install has copied the app, and a second copy of the
# programs left lying around helps no one.
try {
    if (Test-Path $out) { Remove-Item $out -Recurse -Force }
    New-Item -ItemType Directory -Force -Path (Join-Path $out 'font-licenses') | Out-Null
    $built = Join-Path $root "target\$rustTarget\release"
    Copy-Item (Join-Path $built 'led-badge-studio.exe'), (Join-Path $built 'badgectl.exe') $out
    Copy-Item (Join-Path $root 'README.md'), (Join-Path $root 'THIRD_PARTY_NOTICES.txt') $out
    # With an extension, so it opens with a double-click on Windows.
    Copy-Item (Join-Path $root 'LICENSE') (Join-Path $out 'LICENSE.txt')
    New-Item -ItemType Directory -Force -Path (Join-Path $out 'docs') | Out-Null
    Copy-Item (Join-Path $root 'docs\*.md') (Join-Path $out 'docs')
    Copy-Item (Join-Path $root 'assets\fonts\licenses\*') (Join-Path $out 'font-licenses')
    New-Item -ItemType Directory -Force -Path $dist | Out-Null
    # Earlier builds put the folder in dist\ itself.
    $oldOut = Join-Path $dist $name
    if (Test-Path $oldOut) { Remove-Item $oldOut -Recurse -Force }
    $zip = Join-Path $dist "$name.zip"
    if (Test-Path $zip) { Remove-Item $zip -Force }
    Compress-Archive -Path "$out\*" -DestinationPath $zip
    Write-Host "Built $zip"

    if ($Install) {
        Step 'Installing'
        $target = Join-Path $env:LOCALAPPDATA "Programs\$appName"
        New-Item -ItemType Directory -Force -Path $target | Out-Null
        Copy-Item "$out\*" $target -Recurse -Force
        $shortcutPath = Join-Path ([Environment]::GetFolderPath('Programs')) "$appName.lnk"
        $shell = New-Object -ComObject WScript.Shell
        $shortcut = $shell.CreateShortcut($shortcutPath)
        $shortcut.TargetPath = Join-Path $target 'led-badge-studio.exe'
        $shortcut.WorkingDirectory = $target
        $shortcut.Save()

        # Register the .badge file type for the current user.
        $exe = Join-Path $target 'led-badge-studio.exe'
        $classes = 'HKCU:\Software\Classes'
        $progId = 'LedBadgeStudio.Project'
        New-Item -Force -Path "$classes\.badge" -Value $progId | Out-Null
        New-ItemProperty -Force -Path "$classes\.badge" -Name 'Content Type' -Value 'application/x-led-badge' | Out-Null
        New-Item -Force -Path "$classes\$progId" -Value "$appName project" | Out-Null
        # Resource ID 2 is the .badge file icon (build.rs); 1 is the app icon.
        New-Item -Force -Path "$classes\$progId\DefaultIcon" -Value "`"$exe`",-2" | Out-Null
        New-Item -Force -Path "$classes\$progId\shell\open\command" -Value "`"$exe`" `"%1`"" | Out-Null
        # Tell Explorer so icons and double-click update right away.
        Add-Type -Namespace Win32 -Name Shell -MemberDefinition `
            '[DllImport("shell32.dll")] public static extern void SHChangeNotify(int e, int f, IntPtr a, IntPtr b);'
        [Win32.Shell]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
        Write-Host "Installed to $target, with a Start menu shortcut; .badge files now open with it."
    }
} finally {
    # Antivirus can hold a new .exe open for a moment: a folder left behind
    # isn't worth failing the build over.
    if (Test-Path $out) { Remove-Item $out -Recurse -Force -ErrorAction SilentlyContinue }
}
