@echo off
rem Copper 环境启动器（Windows）。用法：
rem   copper build <file.co> [-o 产物.exe]
rem   copper run   <file.co>
rem   copper check <file.co>      静态检查
rem   copper ast   <file.co>      打印 AST
rem   copper json  <file.co>      打印 AST JSON
rem   copper stats <file.co>      打印统计
rem   copper env                  打印环境信息
rem   copper self ...             强制用自举版编译器
setlocal EnableExtensions
set "HERE=%~dp0"
for %%I in ("%HERE%..") do set "COPPER_HOME=%%~fI"
if not defined COPPER_HOME set "COPPER_HOME=%HERE%.."
set "COPPER=%HERE%copper.exe"
set "KIND=env"
if /I "%~1"=="self" ( set "COPPER=%HERE%copper-self.exe" & set "KIND=self" & shift )
if not exist "%COPPER%" set "COPPER=%HERE%copper-self.exe" & set "KIND=self"
if not exist "%COPPER%" echo [copper] 找不到编译器（bin\copper.exe / copper-self.exe）& exit /b 2

if not defined COPPER_RT set "COPPER_RT=%COPPER_HOME%\lib\colang_rt.rs"
if not defined COLANG_CRATE if exist "%COPPER_HOME%\..\scaffold\src-tauri\float-lang\Cargo.toml" for %%I in ("%COPPER_HOME%\..\scaffold\src-tauri\float-lang") do set "COLANG_CRATE=%%~fI"
if not defined COLANG_CARGO if exist "%USERPROFILE%\.cargo\bin\cargo.exe" set "COLANG_CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"

set "CMD=%~1"
if "%CMD%"=="" goto usage
if /I "%CMD%"=="env"      goto do_env
if /I "%CMD%"=="build"    goto do_build
if /I "%CMD%"=="run"      goto do_run
if /I "%CMD%"=="check"    goto do_mode
if /I "%CMD%"=="ast"      goto do_mode
if /I "%CMD%"=="json"     goto do_mode
if /I "%CMD%"=="stats"    goto do_mode
goto usage

:do_env
echo COPPER_HOME = %COPPER_HOME%
echo compiler    = %COPPER%  ^(%KIND%^)
echo runtime     = %COPPER_RT%
echo crate       = %COLANG_CRATE%
echo cargo       = %COLANG_CARGO%
if exist "%COPPER_HOME%\copper.json" type "%COPPER_HOME%\copper.json"
if exist "%COPPER%" for %%I in ("%COPPER%") do echo size        = %%~zI bytes
exit /b 0

:need_target
if "%~2"=="" echo [copper] 缺少源文件 & exit /b 2
for %%I in ("%~2") do set "COLANG_TARGET=%%~fI"
if not exist "%COLANG_TARGET%" echo [copper] 找不到源文件：%COLANG_TARGET% & exit /b 2
exit /b 0

:do_mode
call :need_target %*
set "COLANG_MODE=%CMD%"
"%COPPER%"
exit /b %ERRORLEVEL%

:do_build
call :need_target %*
if not defined COLANG_OUT set "COLANG_OUT=%TEMP%\copper_build"
set "COLANG_MODE=build"
"%COPPER%"
if not exist "%COLANG_OUT%\target\release\app.exe" exit /b 1
if "%~3"=="-o" if not "%~4"=="" (
  copy /y "%COLANG_OUT%\target\release\app.exe" "%~4" >nul
  echo [copper] 产物：%~4
) else (
  echo [copper] 产物：%COLANG_OUT%\target\release\app.exe
)
exit /b 0

:do_run
call :need_target %*
if not defined COLANG_OUT set "COLANG_OUT=%TEMP%\copper_build"
set "COLANG_MODE=build"
"%COPPER%"
if not exist "%COLANG_OUT%\target\release\app.exe" exit /b 1
"%COLANG_OUT%\target\release\app.exe"
exit /b %ERRORLEVEL%

:usage
echo 用法：copper [self] build^|run^|check^|ast^|json^|stats^|env  ^<file.co^>
exit /b 2