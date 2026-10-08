@echo off
rem Builds LED Badge Studio on Windows, installing the build tools and Rust if
rem needed. Double-click to build; from a terminal, "build.cmd --install" also
rem installs it (Start menu shortcut, .badge files open with it), and
rem "--arch x64" or "--arch arm64" builds for that processor instead of this
rem computer's.
setlocal
set "ARGS="
:loop
if "%~1"=="" goto run
if /i "%~1"=="--install" (
    set "ARGS=%ARGS% -Install"
) else if /i "%~1"=="--arch" (
    set "ARGS=%ARGS% -Arch %~2"
    shift
) else (
    set "ARGS=%ARGS% %1"
)
shift
goto loop
:run
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0packaging\windows\build.ps1" %ARGS%
echo.
pause
