# Builds crates/core/data/emoji.txt from Unicode's emoji-test.txt and CLDR annotations (en, uk).
#
#   powershell -File scripts\gen-emoji.ps1 -Source <folder>
#
# <folder> holds emoji-test.txt (https://unicode.org/Public/emoji/15.0/emoji-test.txt) and
# ann-en.json, ann-uk.json, der-en.json, der-uk.json (cldr-annotations-full and
# cldr-annotations-derived-full, annotations/<lang>/annotations.json). One line per emoji, in
# emoji-test order: glyph, English name, English keywords, Ukrainian name, Ukrainian keywords,
# tab-separated, keywords joined by "|". Skin-tone variants are left out.

param([Parameter(Mandatory)][string]$Source)

$ErrorActionPreference = 'Stop'
$out = Join-Path $PSScriptRoot '..\crates\core\data\emoji.txt'

function Read-Json($name) {
    $text = [IO.File]::ReadAllText((Join-Path $Source $name), [Text.Encoding]::UTF8)
    $j = $text | ConvertFrom-Json
    if ($j.annotations) { return $j.annotations.annotations }
    return $j.annotationsDerived.annotations
}

function Merge($a, $b) {
    $h = @{}
    foreach ($o in @($a, $b)) {
        foreach ($p in $o.PSObject.Properties) { $h[$p.Name] = $p.Value }
    }
    $h
}

$en = Merge (Read-Json 'ann-en.json') (Read-Json 'der-en.json')
$uk = Merge (Read-Json 'ann-uk.json') (Read-Json 'der-uk.json')

function Find($table, $glyph) {
    if ($table.ContainsKey($glyph)) { return $table[$glyph] }
    $bare = $glyph.Replace([string][char]0xFE0F, '')
    if ($table.ContainsKey($bare)) { return $table[$bare] }
    $null
}

function Clean([string]$s) { $s.Replace("`t", ' ').Replace('|', ' ').Trim() }

# Keywords that add something: not a word of the name, and not a longer form of another keyword
# (search matches prefixes, so "smil" in "smile" already finds "smiling").
function Keywords($list, [string]$name) {
    $words = $name.ToLower() -split '[\s:,]+'
    $kw = @($list | ForEach-Object { Clean $_ } | Where-Object { $_ -and $_ -ne $name -and ($words -notcontains $_.ToLower()) } | Sort-Object Length)
    $keep = New-Object System.Collections.Generic.List[string]
    foreach ($k in $kw) {
        $lk = $k.ToLower()
        if (-not ($keep | Where-Object { $lk.StartsWith($_.ToLower()) }) -and -not ($words | Where-Object { $_ -and $lk.StartsWith($_) })) { $keep.Add($k) }
    }
    $keep -join '|'
}

$skin = 0x1F3FB..0x1F3FF
$lines = New-Object System.Collections.Generic.List[string]
foreach ($line in [IO.File]::ReadAllLines((Join-Path $Source 'emoji-test.txt'), [Text.Encoding]::UTF8)) {
    if ($line -notmatch '^([0-9A-F ]+?)\s*; fully-qualified') { continue }
    $cps = $Matches[1].Trim() -split ' ' | ForEach-Object { [Convert]::ToInt32($_, 16) }
    if ($cps | Where-Object { $skin -contains $_ }) { continue }
    $glyph = -join ($cps | ForEach-Object { [char]::ConvertFromUtf32($_) })
    $e = Find $en $glyph
    if (-not $e -or -not $e.tts) { continue }
    $u = Find $uk $glyph
    $enName = Clean $e.tts[0]
    $enKw = Keywords $e.default $enName
    if ($u -and $u.tts) {
        $ukName = Clean $u.tts[0]
        $ukKw = Keywords $u.default $ukName
    } else {
        $ukName = $enName
        $ukKw = ''
    }
    $lines.Add("$glyph`t$enName`t$enKw`t$ukName`t$ukKw")
}

[IO.File]::WriteAllText($out, (($lines -join "`n") + "`n"), (New-Object Text.UTF8Encoding $false))
"{0} emoji, {1:N0} bytes -> {2}" -f $lines.Count, (Get-Item $out).Length, (Resolve-Path $out)
