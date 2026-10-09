# Build script 
# Requires: Rust nightly toolchain (for build-std), ARM GNU toolchain (for linker)
param(
    [switch]$Release = $true,
    [switch]$Debug,
    [switch]$Clean
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $ProjectRoot


# ── 3. Verify ARM GNU toolchain linker ──────────────────────────────────────
$armLd = "D:\armeabi_none_toolchain\bin\arm-none-eabi-ld.exe"
if (-not (Test-Path $armLd)) {
    Write-Warning "ARM GNU linker not found at: $armLd"
    Write-Warning "Update arm926ej-s.json 'linker' field with the correct path."
    exit 1
}

# ── 4. Clean if requested ────────────────────────────────────────────────────
if ($Clean) {
    Write-Host "[*] Cleaning target directory..." -ForegroundColor Yellow
    cargo clean
}

# ── 5. Build ─────────────────────────────────────────────────────────────────
$buildProfile = if ($Debug) { "debug" } else { "release" }
$buildFlag = if ($Debug) { "" } else { "--release" }

Write-Host "[*] Building ($buildProfile profile)..." -ForegroundColor Cyan
cargo build $buildFlag -Zjson-target-spec 
Write-Host "[*] Build succeeded." -ForegroundColor Green


# ── 6. Locate output artifacts ──────────────────────────────────────────────
$targetDir = if ($Debug) { "arm926ej-s\debug" } else { "arm926ej-s\release" }
$depsDir = Join-Path $ProjectRoot "target\$targetDir"

$elf = Get-Item "$depsDir\n32903_nand_loader" -ErrorAction SilentlyContinue

if (-not $elf) {
    Write-Error "ELF not found in $depsDir"
    exit 1
}

$elfPath = $elf.FullName
$elfSize = $elf.Length

# ── 7. Convert ELF → binary (using llvm-objcopy from nightly toolchain) ────
$objcopy = "D:\armeabi_none_toolchain\bin\arm-none-eabi-objcopy.exe"

if (Test-Path $objcopy) {
    $binPath = "$depsDir\n32903_nand_loader.bin"
    Write-Host "[*] Generating binary: $binPath" -ForegroundColor Cyan
    & $objcopy -O binary $elf $binPath
    if ($LASTEXITCODE -ne 0) {
        Write-Error "objcopy failed."
        exit 1
    }
    $binSize = (Get-Item $binPath).Length
} else {
    Write-Warning "objcopy not found. Skipping .bin generation."
}

# ── 8. Summary ───────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "Build complete:" -ForegroundColor Green
Write-Host "  Profile : $buildProfile" -ForegroundColor Green
Write-Host "  ELF     : $elfPath ($($elfSize) bytes)" -ForegroundColor Green
if ($binPath) {
    Write-Host "  BIN     : $binPath ($($binSize) bytes)" -ForegroundColor Green
    python prepend_header.py $binPath -o nandloader.bin --exec-addr 0x700000
}

