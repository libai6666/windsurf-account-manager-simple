function Encode-ProtoString($fieldNum, $str) {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($str)
    $tag = [byte](($fieldNum -shl 3) -bor 2)
    $result = New-Object System.Collections.Generic.List[byte]
    $result.Add($tag)
    $l = $bytes.Length
    while ($l -gt 0x7F) { $result.Add([byte](($l -band 0x7F) -bor 0x80)); $l = $l -shr 7 }
    $result.Add([byte]$l)
    foreach($b in $bytes) { $result.Add([byte]$b) }
    return ,$result
}

function Parse-ProtoFields($data) {
    $fields = @{}; $pos = 0
    while ($pos -lt $data.Length) {
        $tag = $data[$pos]; $pos++
        $fn = $tag -shr 3; $wt = $tag -band 7
        if ($wt -eq 2) {
            $len = 0; $shift = 0
            while ($pos -lt $data.Length) {
                $b = $data[$pos]; $pos++
                $len = $len -bor (($b -band 0x7F) -shl $shift)
                if (($b -band 0x80) -eq 0) { break }; $shift += 7
            }
            $fields[$fn] = [System.Text.Encoding]::UTF8.GetString($data, $pos, $len)
            $pos += $len
        } elseif ($wt -eq 0) {
            while ($pos -lt $data.Length -and ($data[$pos] -band 0x80)) { $pos++ }
            $pos++
        } else { break }
    }
    return $fields
}

# Step 1: Login
$loginResp = Invoke-RestMethod -Uri "https://windsurf.com/_devin-auth/password/login" -Method POST -Body '{"email":"nmorrison941@asdascas.dpdns.org","password":"nmorrison941"}' -ContentType "application/json"
$auth1 = $loginResp.token
Write-Host "auth1=$($auth1.Substring(0,30))..."

# Step 2: WindsurfPostAuth
$postAuthBody = Encode-ProtoString 1 $auth1
$resp2 = Invoke-WebRequest -Uri "https://windsurf.com/_backend/exa.seat_management_pb.SeatManagementService/WindsurfPostAuth" -Method POST -Body ([byte[]]$postAuthBody.ToArray()) -ContentType "application/proto" -Headers @{"Connect-Protocol-Version"="1"} -UseBasicParsing
$fields2 = Parse-ProtoFields $resp2.Content
$sessionToken = $fields2[1]
Write-Host "sessionToken=$($sessionToken.Substring(0,40))..."

# Build SubscribeToPlan body  
$body = New-Object System.Collections.Generic.List[byte]
$f1 = Encode-ProtoString 1 $sessionToken
foreach($b in $f1) { $body.Add([byte]$b) }
$body.Add(0x18); $body.Add(0x01)  # start_trial = true
$f4 = Encode-ProtoString 4 "https://windsurf.com/billing/payment-success?plan_tier=pro"
foreach($b in $f4) { $body.Add([byte]$b) }
$f5 = Encode-ProtoString 5 "https://windsurf.com/plan?plan_cancelled=true&plan_tier=pro"
foreach($b in $f5) { $body.Add([byte]$b) }
$body.Add(0x40); $body.Add(0x02)  # teams_tier = 2 (Pro)
$body.Add(0x48); $body.Add(0x01)  # payment_period = 1

Write-Host "`nbody size=$($body.Count) bytes"

# Test 1: web-backend.windsurf.com with x-auth-token
Write-Host "`n--- Test 1: web-backend.windsurf.com + x-auth-token header ---"
try {
    $subResp = Invoke-WebRequest -Uri "https://web-backend.windsurf.com/exa.seat_management_pb.SeatManagementService/SubscribeToPlan" -Method POST -Body ([byte[]]$body.ToArray()) -ContentType "application/proto" -Headers @{"Connect-Protocol-Version"="1"; "x-auth-token"=$sessionToken} -UseBasicParsing
    Write-Host "OK: $($subResp.StatusCode)"
} catch {
    $ex=$_.Exception
    if ($ex.Response) { $sr=[System.IO.StreamReader]::new($ex.Response.GetResponseStream()); Write-Host "FAIL: $($ex.Response.StatusCode.value__) - $($sr.ReadToEnd())" }
    else { Write-Host "ERR: $($ex.Message)" }
}

# Test 2: windsurf.com/_backend with x-auth-token  
Write-Host "`n--- Test 2: windsurf.com/_backend + x-auth-token header ---"
try {
    $subResp2 = Invoke-WebRequest -Uri "https://windsurf.com/_backend/exa.seat_management_pb.SeatManagementService/SubscribeToPlan" -Method POST -Body ([byte[]]$body.ToArray()) -ContentType "application/proto" -Headers @{"Connect-Protocol-Version"="1"; "x-auth-token"=$sessionToken} -UseBasicParsing
    Write-Host "OK: $($subResp2.StatusCode)"
} catch {
    $ex=$_.Exception
    if ($ex.Response) { $sr=[System.IO.StreamReader]::new($ex.Response.GetResponseStream()); Write-Host "FAIL: $($ex.Response.StatusCode.value__) - $($sr.ReadToEnd())" }
    else { Write-Host "ERR: $($ex.Message)" }
}

# Test 3: windsurf.com/_backend with cookie
$cookieJwt = $sessionToken -replace '^devin-session-token\$', ''
Write-Host "`n--- Test 3: windsurf.com/_backend + cookie ---"
try {
    $session = New-Object Microsoft.PowerShell.Commands.WebRequestSession
    $cookie = New-Object System.Net.Cookie("devin-session-token", $cookieJwt, "/", "windsurf.com")
    $session.Cookies.Add($cookie)
    $subResp3 = Invoke-WebRequest -Uri "https://windsurf.com/_backend/exa.seat_management_pb.SeatManagementService/SubscribeToPlan" -Method POST -Body ([byte[]]$body.ToArray()) -ContentType "application/proto" -Headers @{"Connect-Protocol-Version"="1"} -WebSession $session -UseBasicParsing
    Write-Host "OK: $($subResp3.StatusCode)"
} catch {
    $ex=$_.Exception
    if ($ex.Response) { $sr=[System.IO.StreamReader]::new($ex.Response.GetResponseStream()); Write-Host "FAIL: $($ex.Response.StatusCode.value__) - $($sr.ReadToEnd())" }
    else { Write-Host "ERR: $($ex.Message)" }
}

# Test 4: web-backend with Bearer auth1 token
Write-Host "`n--- Test 4: web-backend + Authorization Bearer auth1 ---"
try {
    $subResp4 = Invoke-WebRequest -Uri "https://web-backend.windsurf.com/exa.seat_management_pb.SeatManagementService/SubscribeToPlan" -Method POST -Body ([byte[]]$body.ToArray()) -ContentType "application/proto" -Headers @{"Connect-Protocol-Version"="1"; "Authorization"="Bearer $auth1"} -UseBasicParsing
    Write-Host "OK: $($subResp4.StatusCode)"
} catch {
    $ex=$_.Exception
    if ($ex.Response) { $sr=[System.IO.StreamReader]::new($ex.Response.GetResponseStream()); Write-Host "FAIL: $($ex.Response.StatusCode.value__) - $($sr.ReadToEnd())" }
    else { Write-Host "ERR: $($ex.Message)" }
}
