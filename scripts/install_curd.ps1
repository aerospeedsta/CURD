# CURD Windows Installation & Path Setup Script
# This script copies curd.exe to a local bin directory and adds it to the User PATH.

$InstallDir = "$HOME\.curd\bin"
$BinaryName = "curd.exe"
$SourcePath = "target\release\$BinaryName"

if (!(Test-Path $SourcePath)) {
    $SourcePath = ".\$BinaryName"
}

function Install-Curd {
    if (!(Test-Path $SourcePath)) {
        Write-Error "Error: $BinaryName not found in release folder or current directory. Run 'make release' first."
        return
    }

    Write-Host "--- CURD Windows Installer ---" -ForegroundColor Cyan
    
    if (!(Test-Path $InstallDir)) {
        Write-Host "Creating installation directory: $InstallDir"
        New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    }

    Write-Host "Copying $BinaryName to $InstallDir..."
    Copy-Item -Path $SourcePath -Destination "$InstallDir\$BinaryName" -Force

    # Add to User PATH if not already present
    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($UserPath -notlike "*$InstallDir*") {
        Write-Host "Adding $InstallDir to User PATH..."
        $NewPath = "$UserPath;$InstallDir"
        [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
        $env:Path = "$env:Path;$InstallDir"
        Write-Host "✅ SUCCESS: curd is now in your PATH." -ForegroundColor Green
    } else {
        Write-Host "✅ curd is already in your PATH." -ForegroundColor Green
    }

    Write-Host "`nTo enable command interception (e.g., auto-routing 'cargo build'), run:"
    Write-Host "curd hook powershell | Out-File -FilePath `$PROFILE -Append" -ForegroundColor Yellow
}

function Uninstall-Curd {
    Write-Host "--- CURD Windows Uninstaller ---" -ForegroundColor Red
    
    if (Test-Path $InstallDir) {
        Write-Host "Removing $InstallDir..."
        Remove-Item -Recurse -Force $InstallDir
    }

    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($UserPath -like "*$InstallDir*") {
        Write-Host "Removing $InstallDir from User PATH..."
        $NewPath = $UserPath -replace [regex]::Escape(";$InstallDir"), ""
        $NewPath = $NewPath -replace [regex]::Escape("$InstallDir;"), ""
        [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
        Write-Host "✅ SUCCESS: curd removed from PATH." -ForegroundColor Green
    }

    Write-Host "Note: You may still have the hook in your `$PROFILE. Please remove it manually."
}

# Default action: Install
Install-Curd
