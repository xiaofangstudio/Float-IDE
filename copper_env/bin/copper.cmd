@echo off
rem Copper environment launcher (Windows)
rem   copper build <file.co> [-o out.exe]     AOT compile to native exe
rem   copper run   <file.co>                   build then run
rem   copper check <file.co>                   static check
rem   copper ast|json|stats <file.co>          dump / stats
rem   copper env                              environment info
rem   copper self build <file.co>              force the self-hosted compiler
rem
rem No Rust toolchain? build falls back to a portable COVM exe (single file,
rem double-click to run, no dependencies) instead of dead-ending.
setlocal EnableExtensions
set "HERE=%~dp0"
for %%I in ("%HERE%..") do set "COPPER_HOME=%%~fI"
if not defined COPPER_HOME set "COPPER_HOME=%HERE%.."
rem --- bundled Rust toolchain: package ships it, nothing to install
if exist "%COPPER_HOME%\toolchain\bin\cargo.exe" (
  set "PATH=%COPPER_HOME%\toolchain\bin;%PATH%"
  if not defined CARGO_HOME set "CARGO_HOME=%COPPER_HOME%\cache\cargo"
  if not defined COLANG_CARGO set "COLANG_CARGO=%COPPER_HOME%\toolchain\bin\cargo.exe"
  if not defined RUSTFLAGS set "RUSTFLAGS=-C linker=rust-lld"
)

rem --- compiler resolution: 统一 `colang <版本>.exe`，多份取最新；copper.exe 只作兜底
set "KIND=env"
set "EXE="
for /f "delims=" %%F in ('dir /b /o-n "%HERE%windows-x64\colang *.exe" 2^>nul ^| findstr /v /i "-self"') do if not defined EXE set "EXE=%HERE%windows-x64\%%F"
if not defined EXE for /f "delims=" %%F in ('dir /b /o-n "%HERE%colang *.exe" 2^>nul ^| findstr /v /i "-self"') do if not defined EXE set "EXE=%HERE%%%F"
if not defined EXE set "EXE=%HERE%windows-x64\colang.exe"
if not exist "%EXE%" set "EXE=%HERE%colang.exe"
if not exist "%EXE%" set "EXE=%HERE%windows-x64\copper.exe"
if not exist "%EXE%" set "EXE=%HERE%copper.exe"
set "SELFEXE="
for /f "delims=" %%F in ('dir /b /o-n "%HERE%windows-x64\colang-self *.exe" 2^>nul') do if not defined SELFEXE set "SELFEXE=%HERE%windows-x64\%%F"
if not defined SELFEXE set "SELFEXE=%HERE%windows-x64\copper-self.exe"
if not exist "%SELFEXE%" set "SELFEXE=%HERE%copper-self.exe"
set "COPPER=%EXE%"
if /I "%~1"=="self" goto pick_self
goto pick_done

rem "self build file.co" -> drop the word self so %1 becomes the real command
:pick_self
set "KIND=self"
set "COPPER=%SELFEXE%"
shift
goto pick_done

:pick_done
if exist "%COPPER%" goto have_compiler
if exist "%EXE%" set "COPPER=%EXE%" & set "KIND=env" & goto have_compiler
if exist "%SELFEXE%" set "COPPER=%SELFEXE%" & set "KIND=self" & goto have_compiler
echo [copper] compiler not found: bin\windows-x64\copper.exe / bin\copper.exe
exit /b 2

:have_compiler
if not defined COPPER_RT set "COPPER_RT=%COPPER_HOME%\lib\colang_rt.rs"
if not defined COLANG_CRATE if exist "%COPPER_HOME%\share\float-lang\Cargo.toml" for %%I in ("%COPPER_HOME%\share\float-lang") do set "COLANG_CRATE=%%~fI"
if not defined COLANG_CRATE if exist "%COPPER_HOME%\..\scaffold\src-tauri\float-lang\Cargo.toml" for %%I in ("%COPPER_HOME%\..\scaffold\src-tauri\float-lang") do set "COLANG_CRATE=%%~fI"
if not defined COLANG_CARGO if exist "%USERPROFILE%\.cargo\bin\cargo.exe" set "COLANG_CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"
if not defined COLANG_CARGO set "COLANG_CARGO=cargo"

set "EMBED=%HERE%windows-x64\copper-embed.exe"
if not exist "%EMBED%" set "EMBED=%HERE%copper-embed.exe"
set "RUNEXE=%HERE%windows-x64\copper-run.exe"
if not exist "%RUNEXE%" set "RUNEXE=%HERE%copper-run.exe"

set "CMD=%~1"
if "%CMD%"=="" goto usage
if /I "%CMD%"=="env"   goto do_env
if /I "%CMD%"=="build" goto do_build
if /I "%CMD%"=="run"   goto do_run
if /I "%CMD%"=="check" goto do_mode
if /I "%CMD%"=="ast"   goto do_mode
if /I "%CMD%"=="json"  goto do_mode
if /I "%CMD%"=="stats" goto do_mode
goto usage

:do_env
echo COPPER_HOME = %COPPER_HOME%
echo compiler    = %COPPER%  (%KIND%)
echo runtime     = %COPPER_RT%
echo crate       = %COLANG_CRATE%
echo cargo       = %COLANG_CARGO%
echo embed       = %EMBED%
if exist "%COPPER_HOME%\copper.json" type "%COPPER_HOME%\copper.json"
if exist "%COPPER%" for %%I in ("%COPPER%") do echo size        = %%~zI bytes
exit /b 0

:need_target
if "%~2"=="" goto no_target
for %%I in ("%~2") do set "COLANG_TARGET=%%~fI"
if not exist "%COLANG_TARGET%" goto bad_target
exit /b 0

:no_target
echo [copper] missing source file
exit /b 2

:bad_target
echo [copper] source not found: %COLANG_TARGET%
exit /b 2

:do_mode
call :need_target %1 %2 %3 %4 %5
set "COLANG_MODE=%CMD%"
"%COPPER%"
exit /b %ERRORLEVEL%

:do_build
call :need_target %1 %2 %3 %4 %5
if not defined COLANG_OUT set "COLANG_OUT=%TEMP%\copper_build"
where cargo >nul 2>nul || goto embed_build
set "COLANG_MODE=build"
"%COPPER%"
if not exist "%COLANG_OUT%\target\release\app.exe" goto aot_failed
goto build_copy

:embed_build
if not exist "%EMBED%" goto no_embed
if not exist "%COLANG_OUT%\target\release" mkdir "%COLANG_OUT%\target\release"
set "COPPER_RUN=%RUNEXE%"
"%EMBED%" "%COLANG_TARGET%" "%COLANG_OUT%\target\release\app.exe"
if errorlevel 1 exit /b 1
echo [copper] no Rust toolchain found: built a portable COVM exe (works everywhere, slower than AOT)
goto build_copy

:no_embed
echo [copper] no Rust toolchain and no portable builder (copper-embed.exe) - cannot build
echo [copper] install Rust from https://rustup.rs to get the fast AOT build
exit /b 3

:aot_failed
echo [copper] AOT step failed, falling back to a portable exe
goto embed_build

:build_copy
if "%~3"=="-o" if not "%~4"=="" goto copy_out
echo [copper] output: %COLANG_OUT%\target\release\app.exe
exit /b 0

:copy_out
copy /y "%COLANG_OUT%\target\release\app.exe" "%~4" >nul
echo [copper] output: %~4
exit /b 0

:do_run
call :need_target %1 %2 %3 %4 %5
if not defined COLANG_OUT set "COLANG_OUT=%TEMP%\copper_build"
where cargo >nul 2>nul || goto embed_run
set "COLANG_MODE=build"
"%COPPER%"
if not exist "%COLANG_OUT%\target\release\app.exe" goto aot_failed
"%COLANG_OUT%\target\release\app.exe"
exit /b %ERRORLEVEL%

:embed_run
if not exist "%EMBED%" goto no_embed
if not exist "%COLANG_OUT%\target\release" mkdir "%COLANG_OUT%\target\release"
set "COPPER_RUN=%RUNEXE%"
"%EMBED%" "%COLANG_TARGET%" "%COLANG_OUT%\target\release\app.exe"
if errorlevel 1 exit /b 1
"%COLANG_OUT%\target\release\app.exe"
exit /b %ERRORLEVEL%

:usage
echo usage: copper [self] build^|run^|check^|ast^|json^|stats^|env  ^<file.co^>
exit /b 2
