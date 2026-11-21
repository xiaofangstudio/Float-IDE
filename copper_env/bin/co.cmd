@echo off
rem Copper launcher: type "co" like "python"
setlocal EnableExtensions
set "HERE=%~dp0"
for %%I in ("%HERE%..") do set "COPPER_HOME=%%~fI"
set "PLAT=windows-x64"
set "EXE="
for /f "delims=" %%F in ('dir /b /o-n "%HERE%%PLAT%\colang *.exe" 2^>nul ^| findstr /v /i /c:"-self"') do if not defined EXE set "EXE=%HERE%%PLAT%\%%F"
if not defined EXE for /f "delims=" %%F in ('dir /b /o-n "%HERE%colang *.exe" 2^>nul ^| findstr /v /i /c:"-self"') do if not defined EXE set "EXE=%HERE%%%F"
if not defined EXE set "EXE=%HERE%%PLAT%\colang.exe"
if not exist "%EXE%" set "EXE=%HERE%colang.exe"
if not exist "%EXE%" set "EXE=%HERE%copper.exe"
set "SELF="
for /f "delims=" %%F in ('dir /b /o-n "%HERE%%PLAT%\colang-self *.exe" 2^>nul') do if not defined SELF set "SELF=%HERE%%PLAT%\%%F"
if not defined SELF set "SELF=%HERE%%PLAT%\copper-self.exe"
if not exist "%SELF%" set "SELF=%HERE%copper-self.exe"
set "RUN=%HERE%%PLAT%\copper-run.exe"
if not exist "%RUN%" set "RUN=%HERE%copper-run.exe"
set "RT=%COPPER_HOME%\lib\colang_rt.rs"
set "VER=1.0.0"
set "VERRAW="
for /f "tokens=2 delims=:," %%A in ('findstr /i /c:"version" "%COPPER_HOME%\copper.json" 2^>nul') do if not defined VERRAW set "VERRAW=%%A"
if defined VERRAW set "VER=%VERRAW: =%"
if defined VER set "VER=%VER:"=%"

if "%~1"=="" goto banner
if /I "%~1"=="--version" goto version
if /I "%~1"=="-v" goto version
if /I "%~1"=="-V" goto version
if /I "%~1"=="version" goto version
if /I "%~1"=="help" goto helpfull
if /I "%~1"=="-h" goto usage
if /I "%~1"=="--help" goto usage
if /I "%~1"=="env" goto delege
if /I "%~1"=="build" goto delege
if /I "%~1"=="check" goto delege
if /I "%~1"=="ast" goto delege
if /I "%~1"=="json" goto delege
if /I "%~1"=="stats" goto delege
if /I "%~1"=="self" goto delege
if /I "%~1"=="run" goto run2
goto run1

:delege
call "%HERE%copper.cmd" %1 %2 %3 %4 %5
exit /b %ERRORLEVEL%

:run1
if not exist "%~1" goto nofile
set "SCRIPT=%~1"
set "INP=%~2"
goto runnow

:run2
if "%~2"=="" goto nofile2
if not exist "%~2" goto nofile2
set "SCRIPT=%~2"
set "INP=%~3"
goto runnow

:runnow
if not exist "%RUN%" goto norun
set "COPPER_RT=%RT%"
"%RUN%" "%SCRIPT%" "%INP%"
exit /b %ERRORLEVEL%

:version
echo Copper %VER%  (%PLAT% / COVM + CO AOT)
echo   interpreter : %RUN%
echo   compiler    : %EXE%
echo   self-host   : %SELF%
echo   runtime     : %RT%
echo   home        : %COPPER_HOME%
exit /b 0

:banner
echo Copper %VER%  (%PLAT% / COVM + CO AOT)
echo.
echo usage:
echo   co script.co [input]        run with the COVM interpreter
echo   co run script.co            same
echo   co build script.co [-o out.exe]   compile to a native exe (CO AOT)
echo   co check script.co          static check only
echo   co ast / json / stats script.co  AST / JSON / stats
echo   co env                      environment details
echo   co self cmd                 use the self-hosted compiler
echo   co --version                version only
echo   co help                     full manual (chapters 1-9)
echo.
echo examples:
echo   co language\samples\01_hello.co
echo   co build language\samples\01_hello.co -o hello.exe
rem like python: no args -> banner then interactive prompt
if exist "%RUN%" ( "%RUN%" --repl )
exit /b 0

:helpfull
if not exist "%RUN%" goto norun
"%RUN%" --help
exit /b %ERRORLEVEL%

:usage
call :banner
exit /b 0

:nofile
echo [co] script not found: %~1
exit /b 2

:nofile2
echo [co] usage: co run script.co
exit /b 2

:norun
echo [co] interpreter not found: %RUN%
exit /b 2
