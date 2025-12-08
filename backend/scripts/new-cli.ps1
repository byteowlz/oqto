#!/usr/bin/env pwsh

[CmdletBinding()]
param(
    [Parameter(Position = 0, Mandatory = $true)]
    [ValidatePattern('^[a-zA-Z][a-zA-Z0-9_-]*$')]
    [string]$Name,

    [Parameter()]
    [string]$Path
)

function Write-Usage {
    Write-Host "Usage: new-cli.ps1 <name> [-Path <destination>]" -ForegroundColor Cyan
    Write-Host "Creates a new CLI project by cloning this template." -ForegroundColor Cyan
}

try {
    if ($PSBoundParameters.ContainsKey('Name') -eq $false) {
        Write-Usage
        throw 'Project name is required.'
    }

    $scriptDir = Split-Path -Parent $PSCommandPath
    $templateRoot = Split-Path -Parent $scriptDir

    if ([string]::IsNullOrWhiteSpace($Path)) {
        $parentDir = Split-Path -Parent $templateRoot
        $destination = Join-Path -Path $parentDir -ChildPath $Name
    } else {
        $destination = if ([System.IO.Path]::IsPathRooted($Path)) { $Path } else { Join-Path -Path (Get-Location) -ChildPath $Path }
    }

    if (Test-Path -LiteralPath $destination) {
        throw "Destination already exists: $destination"
    }

    New-Item -ItemType Directory -Path $destination | Out-Null

    $excluded = @('.git', 'target', '.DS_Store')

    function Copy-Template {
        param(
            [string]$Source,
            [string]$Dest
        )

        Get-ChildItem -LiteralPath $Source -Force | ForEach-Object {
            if ($excluded -contains $_.Name) {
                return
            }

            $targetPath = Join-Path -Path $Dest -ChildPath $_.Name

            if ($_.PSIsContainer) {
                if (-not (Test-Path -LiteralPath $targetPath)) {
                    New-Item -ItemType Directory -Path $targetPath | Out-Null
                }
                Copy-Template -Source $_.FullName -Dest $targetPath
            } else {
                Copy-Item -LiteralPath $_.FullName -Destination $targetPath -Force
            }
        }
    }

    Copy-Template -Source $templateRoot -Dest $destination

    $replacements = @(
        'Cargo.toml',
        'Cargo.lock',
        'README.md',
        'examples/config.toml'
    )

    foreach ($relative in $replacements) {
        $filePath = Join-Path -Path $destination -ChildPath $relative
        if (Test-Path -LiteralPath $filePath) {
            $content = Get-Content -LiteralPath $filePath -Raw
            $upperName = $Name.ToUpper() -replace '-', '_'
            $updated = $content -replace 'rust-cli', $Name -replace 'RUST_CLI', $upperName
            Set-Content -LiteralPath $filePath -Value $updated -Encoding UTF8
        }
    }

    Write-Host "Created CLI project at $destination" -ForegroundColor Green
    exit 0
}
catch {
    Write-Error $_
    Write-Usage
    exit 1
}
