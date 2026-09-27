param(
    [string]$PackageName = "TackleCast-Rust",
    [switch]$Zip,
    [switch]$SkipGpuDlls,
    [string]$CudaDllDir
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-Root {
    Split-Path -Path $PSScriptRoot -Parent
}

function Ensure-Exists {
    param([string]$Path, [string]$Message)
    if (-not (Test-Path -LiteralPath $Path)) {
        throw $Message
    }
}

$root = Resolve-Root
$distRoot = Join-Path $root "dist"
$outDir = Join-Path $distRoot $PackageName
$releaseExe = Join-Path $root "target\release\tacklecast.exe"
$settingsSrc = Join-Path $root "tacklecast_settings.json"
$iconSrc = Join-Path $root "assets\icon.ico"
$ffmpegDir = $env:FFMPEG_DIR

if ([string]::IsNullOrWhiteSpace($ffmpegDir)) {
    if (Test-Path "C:\ffmpeg") {
        $ffmpegDir = "C:\ffmpeg"
    } else {
        throw "FFMPEG_DIR is not set. Set it to your FFmpeg root (contains bin/, lib/, include/)."
    }
}

$ffmpegBin = Join-Path $ffmpegDir "bin"
Ensure-Exists -Path $ffmpegBin -Message "FFMPEG_DIR\bin not found: $ffmpegBin"

Push-Location $root
try {
    Write-Host "Building release binary..."
    cargo build --release

    Ensure-Exists -Path $releaseExe -Message "Release executable missing: $releaseExe"
    Ensure-Exists -Path $iconSrc -Message "Icon missing: $iconSrc"

    if (Test-Path -LiteralPath $outDir) {
        Remove-Item -LiteralPath $outDir -Recurse -Force
    }

    New-Item -ItemType Directory -Path $outDir | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $outDir "assets") | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $outDir "logs") | Out-Null

    Copy-Item -LiteralPath $releaseExe -Destination (Join-Path $outDir "TackleCast.exe")
    Copy-Item -LiteralPath $iconSrc -Destination (Join-Path $outDir "assets\icon.ico")

    if (Test-Path -LiteralPath $settingsSrc) {
        Copy-Item -LiteralPath $settingsSrc -Destination (Join-Path $outDir "tacklecast_settings.json")
    }

    $dllPatterns = @(
        "avcodec-*.dll",
        "avformat-*.dll",
        "avdevice-*.dll",
        "avutil-*.dll",
        "swresample-*.dll",
        "swscale-*.dll",
        "avfilter-*.dll",
        "postproc-*.dll"
    )

    foreach ($pattern in $dllPatterns) {
        $matches = Get-ChildItem -Path $ffmpegBin -Filter $pattern -File
        if (-not $matches) {
            throw "Missing FFmpeg DLL pattern '$pattern' in $ffmpegBin"
        }
        foreach ($dll in $matches) {
            Copy-Item -LiteralPath $dll.FullName -Destination (Join-Path $outDir $dll.Name)
        }
    }

    # nvJPEG + CUDA runtime, so GPU decode works on a machine without the CUDA
    # Toolkit installed. Without these the app still runs, but silently falls
    # back to software decode - which defeats the point of testing the GPU path.
    #
    # nvcuda.dll is deliberately NOT bundled: it ships with the NVIDIA display
    # driver and has to match the driver on the target machine.
    $gpuDllsCopied = @()
    if (-not $SkipGpuDlls) {
        $searchDirs = New-Object System.Collections.Generic.List[string]
        if ($CudaDllDir) {
            $searchDirs.Add($CudaDllDir)
        }
        if ($env:CUDA_PATH) {
            $searchDirs.Add((Join-Path $env:CUDA_PATH "bin\x64"))
            $searchDirs.Add((Join-Path $env:CUDA_PATH "bin"))
        }
        $cudaRoot = "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA"
        if (Test-Path -LiteralPath $cudaRoot) {
            # Newest toolkit first.
            Get-ChildItem -LiteralPath $cudaRoot -Directory |
                Sort-Object Name -Descending |
                ForEach-Object {
                    $searchDirs.Add((Join-Path $_.FullName "bin\x64"))
                    $searchDirs.Add((Join-Path $_.FullName "bin"))
                }
        }

        # Also search parent/ancestor folders for any existing TackleCast release folders with CUDA DLLs
        $ancestor = Split-Path -Path $root -Parent
        for ($i = 0; $i -lt 3; $i++) {
            if (-not $ancestor -or -not (Test-Path -LiteralPath $ancestor)) { break }
            Get-ChildItem -LiteralPath $ancestor -Directory -Filter "TackleCast*" -ErrorAction SilentlyContinue |
                ForEach-Object { $searchDirs.Add($_.FullName) }
            $ancestor = Split-Path -Path $ancestor -Parent
        }

        foreach ($pattern in @("nvjpeg64_*.dll", "cudart64_*.dll")) {
            $found = $null
            foreach ($dir in $searchDirs) {
                if (-not (Test-Path -LiteralPath $dir)) { continue }
                $hit = Get-ChildItem -LiteralPath $dir -Filter $pattern -File -ErrorAction SilentlyContinue |
                    Select-Object -First 1
                if ($hit) { $found = $hit; break }
            }
            if ($found) {
                Copy-Item -LiteralPath $found.FullName -Destination (Join-Path $outDir $found.Name)
                $gpuDllsCopied += $found.Name
                Write-Host "Bundled GPU DLL: $($found.Name)"
            } else {
                Write-Warning "Could not find '$pattern' - GPU decode will fall back to software on machines without the CUDA Toolkit. Searched: $($searchDirs -join '; ')"
            }
        }
    }

    if ($gpuDllsCopied.Count -gt 0) {
        $gpuNote = @"

GPU decode
----------
Bundled: $($gpuDllsCopied -join ', ')

GPU (nvJPEG) decode needs an NVIDIA GPU and a recent driver. These DLLs are from
the CUDA 13 runtime, which requires NVIDIA driver 570 or newer. On an older
driver the app logs that CUDA is unavailable and falls back to software decode -
it still works, just with more CPU load.

To confirm which path is active, open logs\ and look for:
  "nvJPEG GPU decoder initialized in zero-copy mode"   <- best case
  "nvJPEG GPU decoder initialized"                     <- GPU decode, host copy
  "nvcuda.dll not found" / "GPU decode unavailable"    <- software decode
"@
    } else {
        $gpuNote = @"

GPU decode
----------
No CUDA DLLs were bundled, so GPU decode only works if the target machine has
the CUDA Toolkit installed. Otherwise the app falls back to software decode.
"@
    }

    $readmePath = Join-Path $outDir "README.txt"
    @"
TackleCast
==========

Requirements: Windows 10/11 64-bit. Everything needed is in this folder - no
installer, nothing to put on PATH. Keep the files together.

Running it
----------
1. Run TackleCast.exe.
2. Press Escape to open settings; pick your capture device, resolution, and FPS.
3. Press F11 for fullscreen. Escape closes the menu.

The mouse cursor hides after 3 seconds of stillness while the menu is closed.
The display is kept awake while frames are arriving and the window is visible.

Settings are saved to tacklecast_settings.json next to the exe when you exit.
Deleting that file resets everything to defaults.
$gpuNote

Scaling filters
---------------
Settings has a Scaling Filter option: Bilinear, Bicubic, or Lanczos. It only
affects the image when the window is larger than the capture resolution. Tick
"Include Scaling Filter In Overlay" to see which one is live while comparing.

Color space and range
---------------------
Settings includes Color Space (Auto, BT.709, BT.601, BT.2020) and Color Range
(Auto, Limited, Full) options under the Video section. By default, Auto selects
the standard space and range based on capture format and resolution (e.g. BT.709
limited for HD/4K NV12, full for MJPEG). You can override them to match your
source console or capture card signal.

Video formats and dynamic hardware filtering
--------------------------------------------
Settings includes a Video Format selector (Auto, MJPEG, NV12, YUY2, UYVY,
YUV12/YUV420P). TackleCast dynamically probes your capture device's DirectShow
pin capabilities, automatically filtering the Resolution, Frame Rate, and Video
Format dropdowns to only display options genuinely supported by your hardware
at the active configuration. Auto-sanitization ensures settings stay valid if
you change resolution or frame rate.

If something goes wrong
-----------------------
- Log files are in the logs\ folder, newest last. They record the negotiated
  capture format, decode path, frame rate, and any errors. Send the newest one.
- A 30-second summary line reports rendered/uploaded FPS and GPU temperature.
- If capture fails, first check the card appears in Windows Camera or Device
  Manager, and that nothing else (OBS, Camera app, a browser tab) is already
  using it - most capture cards allow only one consumer at a time.
- If the picture is black but FPS is counting up, try a different resolution or
  frame rate; the log line starting "negotiated" shows what the card accepted.
"@ | Set-Content -Path $readmePath -NoNewline

    if ($Zip) {
        $zipPath = Join-Path $distRoot "$PackageName.zip"
        if (Test-Path -LiteralPath $zipPath) {
            Remove-Item -LiteralPath $zipPath -Force
        }
        Compress-Archive -Path (Join-Path $outDir "*") -DestinationPath $zipPath
        Write-Host "Created zip: $zipPath"
    }

    Write-Host "Package ready: $outDir"
}
finally {
    Pop-Location
}
