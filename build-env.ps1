# 导入 MSVC 编译环境到当前 PowerShell 会话
# 用法: . .\build-env.ps1
$vcvars = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"

$tempFile = [System.IO.Path]::GetTempFileName()
cmd /c "`"$vcvars`" && set > `"$tempFile`"" 2>$null
Get-Content $tempFile | ForEach-Object {
    if ($_ -match '^([^=]+)=(.*)$') {
        Set-Item -Path "env:$($matches[1])" -Value $matches[2]
    }
}
Remove-Item $tempFile -ErrorAction SilentlyContinue

# 确保 cargo 在 PATH 中
if ($env:PATH -notlike "*$cargoBin*") {
    $env:PATH = "$cargoBin;$env:PATH"
}

Write-Host "MSVC 编译环境已就绪 (link.exe 可用)" -ForegroundColor Green
