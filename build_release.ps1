# Build release binary and embed multi-resolution app icon
Write-Host "Compiling Window Display Swapper (release)..." -ForegroundColor Cyan
& "$env:USERPROFILE\.cargo\bin\cargo.exe" build --release

if ($LASTEXITCODE -ne 0) {
    Write-Error "Cargo build failed."
    exit $LASTEXITCODE
}

Write-Host "Packaging app.ico into window-display-swapper.exe..." -ForegroundColor Cyan
& powershell -ExecutionPolicy Bypass -File "C:\Users\bookamp\.gemini\antigravity-ide\brain\0a4a7b7d-2f4d-4852-83de-453f9f1b13d7\scratch\inject_icon.ps1"

Write-Host "Done! Binary is ready at target\release\window-display-swapper.exe with embedded icon." -ForegroundColor Green
