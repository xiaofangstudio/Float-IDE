# 把 Copper 环境铺到用户目录（默认 %APPDATA%\Float\copper），并让命令行里的 `co` 立即可用。
#
#   .\install.ps1
#   .\install.ps1 -Dest "D:\Copper"
#   .\install.ps1 -Dest "D:\Copper" -NoPath
#
# 说明：`co` 启动器在 bin\ 根目录，编译器在 bin\windows-x64\；
# 两个目录都会加进用户 PATH，所以装完重开终端敲 `co` 就能进 REPL。
param(
    [Alias("Home")]
    [string]$Dest = "",
    [switch]$NoPath,
    # 兼容旧文档里的写法；现在默认就会加 PATH
    [switch]$AddToPath,
    [switch]$SetHomeVar
)

$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$src = Join-Path $here "copper_env"
if (-not (Test-Path (Join-Path $src "copper.json"))) {
    # 允许直接在 copper_env 目录里跑这个脚本
    if (Test-Path (Join-Path $here "copper.json")) {
        $src = $here
    } else {
        throw "这个包里没找到 copper_env/copper.json，不是有效的 Copper 环境包。"
    }
}

if ([string]::IsNullOrWhiteSpace($Dest)) {
    $Dest = Join-Path $env:APPDATA "Float\copper"
}
Write-Host "安装到：$Dest"
New-Item -ItemType Directory -Force -Path $Dest | Out-Null
$fullSrc = (Resolve-Path $src).Path.TrimEnd('\')
$fullDest = (Resolve-Path $Dest).Path.TrimEnd('\')
if ($fullSrc -ne $fullDest) {
    Copy-Item -Path (Join-Path $src "*") -Destination $Dest -Recurse -Force
} else {
    Write-Host "已经在目标目录，跳过复制。"
}

$plat = "windows-x64"
$binRoot = Join-Path $Dest "bin"
$binDir = Join-Path $binRoot $plat
if (-not (Test-Path $binDir)) { $binDir = $binRoot }

# 版本号最大的那份 = 当前生效的编译器
$compiler = Get-ChildItem $binDir -Filter "colang *.exe" -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -notlike "colang-self*" } |
    Sort-Object { try { [version]((($_.BaseName -replace 'colang ', '') -replace '[^0-9.]', '')) } catch { [version]"0.0.0" } } -Descending |
    Select-Object -First 1
if ($null -eq $compiler) { throw "装完了但 bin 下没找到 colang x.x.xxx.exe。" }

Write-Host "编译器：$($compiler.FullName)"

# PATH：bin 根（co.cmd / copper.cmd）+ 平台目录（colang*.exe / copper-run.exe）
$dirs = @($binRoot)
if ($binDir -ne $binRoot) { $dirs += $binDir }
if (-not $NoPath) {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($null -eq $userPath) { $userPath = "" }
    $parts = $userPath -split ';' | Where-Object { $_ -ne "" }
    # 先把 Copper 的目录从旧位置摘掉，再整体放到最前面：
    # 机器上可能还留着仓库路径或旧安装路径，不能让它们抢先。
    $rest = $parts | Where-Object { $dirs -notcontains $_ }
    $newParts = @($dirs) + @($rest)
    [Environment]::SetEnvironmentVariable("Path", ($newParts -join ';'), "User")
    foreach ($d in $dirs) {
        if (($env:Path -split ';') -notcontains $d) { $env:Path = $d + ";" + $env:Path.TrimStart(';') }
    }
    Write-Host "已把 Copper 的 bin 放到用户 PATH 最前："
    foreach ($d in $dirs) { Write-Host "  $d" }
}

# 自定义目录必须设 COPPER_HOME，IDE 才找得到；默认目录也设上，换目录后不残留旧值
[Environment]::SetEnvironmentVariable("COPPER_HOME", $Dest, "User")
Write-Host "已设 COPPER_HOME=$Dest"

Write-Host ""
Write-Host "装好了。重开一个终端，验证："
Write-Host "  co --version"
Write-Host "  co          # 进交互模式（像 python 一样）"
Write-Host "然后打开 Float IDE：设置 → Copper 环境，应显示「已找到」。"
