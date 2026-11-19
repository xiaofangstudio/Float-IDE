# 把 Copper 环境铺到用户目录（默认 %APPDATA%\Float\copper），装完 Float IDE 就能找到。
#
#   .\install.ps1
#   .\install.ps1 -Home "D:\Copper" -AddToPath -SetHomeVar
param(
    [string]$Dest = "",
    [switch]$AddToPath,
    [switch]$SetHomeVar
)

$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$src = Join-Path $here "copper_env"
if (-not (Test-Path (Join-Path $src "copper.json"))) {
    throw "这个包里没找到 copper_env/copper.json，不是有效的 Copper 环境包。"
}

if ([string]::IsNullOrWhiteSpace($Dest)) {
    $Dest = Join-Path $env:APPDATA "Float\copper"
}
Write-Host "安装到：$Dest"
New-Item -ItemType Directory -Force -Path $Dest | Out-Null
Copy-Item -Path (Join-Path $src "*") -Destination $Dest -Recurse -Force

$plat = "windows-x64"
$binDir = Join-Path $Dest "bin\$plat"
if (-not (Test-Path $binDir)) { $binDir = Join-Path $Dest "bin" }

# 版本号最大的那份 = 当前生效的编译器
$compiler = Get-ChildItem $binDir -Filter "colang *.exe" -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -notlike "colang-self*" } |
    Sort-Object { try { [version]((($_.BaseName -replace 'colang ', '') -replace '[^0-9.]', '')) } catch { [version]"0.0.0" } } -Descending |
    Select-Object -First 1
if ($null -eq $compiler) { throw "装完了但 bin 下没找到 colang x.x.xxx.exe。" }

Write-Host "编译器：$($compiler.FullName)"

if ($AddToPath) {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($userPath -split ';' -notcontains $binDir) {
        [Environment]::SetEnvironmentVariable("Path", ($userPath.TrimEnd(';') + ";" + $binDir), "User")
        Write-Host "已把 $binDir 加进用户 PATH（新开的终端生效）"
    } else {
        Write-Host "PATH 里已经有 $binDir 了"
    }
}
if ($SetHomeVar) {
    [Environment]::SetEnvironmentVariable("COPPER_HOME", $Dest, "User")
    Write-Host "已设 COPPER_HOME=$Dest"
}

Write-Host ""
Write-Host "装好了。验证："
Write-Host "  & ""$($compiler.FullName)""   # 文件名里的 1.9.2 就是版本号（不认识 --version）"
Write-Host "然后打开 Float IDE：设置 → Copper 环境，应显示「已找到」。"
