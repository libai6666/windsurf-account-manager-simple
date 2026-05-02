# Proper protobuf encoding 
function Encode-Proto {
    param([int]$fieldNum, [string]$str)
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($str)
    $tag = [byte](($fieldNum -shl 3) -bor 2)
    $out = [System.Collections.Generic.List[byte]]::new()
    $out.Add($tag)
    $l = $bytes.Length
    while ($l -gt 0x7F) { $out.Add([byte](($l -band 0x7F) -bor 0x80)); $l = $l -shr 7 }
    $out.Add([byte]$l)
    $out.AddRange([byte[]]$bytes)
    return [byte[]]$out.ToArray()
}

# Step 1: Login
Write-Host "Step 1: Login..."
$loginResp = Invoke-RestMethod -Uri "https://windsurf.com/_devin-auth/password/login" -Method POST -Body '{"email":"nmorrison941@asdascas.dpdns.org","password":"nmorrison941"}' -ContentType "application/json"
$auth1 = $loginResp.token
Write-Host "  auth1=$($auth1.Substring(0,30))..."

# Step 2: WindsurfPostAuth
Write-Host "Step 2: WindsurfPostAuth..."
$postBody = Encode-Proto 1 $auth1
$resp2 = Invoke-WebRequest -Uri "https://windsurf.com/_backend/exa.seat_management_pb.SeatManagementService/WindsurfPostAuth" -Method POST -Body $postBody -ContentType "application/proto" -Headers @{"Connect-Protocol-Version"="1"} -UseBasicParsing
$raw = [byte[]]$resp2.Content
# Parse field 1 (session_token)
$pos = 1; $slen = 0; $shift = 0
while ($true) { $b = $raw[$pos]; $pos++; $slen = $slen -bor (($b -band 0x7F) -shl $shift); if (($b -band 0x80) -eq 0) { break }; $shift += 7 }
$sessionToken = [System.Text.Encoding]::UTF8.GetString($raw, $pos, $slen)
Write-Host "  sessionToken=$($sessionToken.Substring(0,40))... (len=$($sessionToken.Length))"

# Build proper body
$body = [System.Collections.Generic.List[byte]]::new()
# field 1: auth_token = session_token
$f1 = Encode-Proto 1 $sessionToken
$body.AddRange([byte[]]$f1)
# field 3: start_trial = true
$body.Add(0x18); $body.Add(0x01)
# field 4: success_url
$f4 = Encode-Proto 4 "https://windsurf.com/billing/payment-success?plan_tier=pro"
$body.AddRange([byte[]]$f4)
# field 5: cancel_url
$f5 = Encode-Proto 5 "https://windsurf.com/plan?plan_cancelled=true&plan_tier=pro"
$body.AddRange([byte[]]$f5)
# field 8: teams_tier = 2 (Pro)
$body.Add(0x40); $body.Add(0x02)
# field 9: payment_period = 1
$body.Add(0x48); $body.Add(0x01)

$bodyBytes = [byte[]]$body.ToArray()
Write-Host "body size=$($bodyBytes.Length) bytes"

function DoTest($name, $url, $headers) {
    Write-Host "`n--- $name ---"
    try {
        $r = Invoke-WebRequest -Uri $url -Method POST -Body $bodyBytes -ContentType "application/proto" -Headers $headers -UseBasicParsing
        Write-Host "OK: $($r.StatusCode) ($($r.Content.Length) bytes)"
    } catch {
        $ex=$_.Exception
        if ($ex.Response) {
            $sr=[System.IO.StreamReader]::new($ex.Response.GetResponseStream())
            Write-Host "FAIL: $($ex.Response.StatusCode.value__) - $($sr.ReadToEnd())"
        } else { Write-Host "ERR: $($ex.Message)" }
    }
}

# Test A: web-backend + x-auth-token only (current code)
DoTest "A: web-backend + x-auth-token(session)" "https://web-backend.windsurf.com/exa.seat_management_pb.SeatManagementService/SubscribeToPlan" @{"Connect-Protocol-Version"="1"; "x-auth-token"=$sessionToken}

# Test B: web-backend + Authorization Bearer auth1  
DoTest "B: web-backend + Bearer auth1" "https://web-backend.windsurf.com/exa.seat_management_pb.SeatManagementService/SubscribeToPlan" @{"Connect-Protocol-Version"="1"; "Authorization"="Bearer $auth1"}

# Test C: web-backend + both headers
DoTest "C: web-backend + x-auth-token + Bearer auth1" "https://web-backend.windsurf.com/exa.seat_management_pb.SeatManagementService/SubscribeToPlan" @{"Connect-Protocol-Version"="1"; "x-auth-token"=$sessionToken; "Authorization"="Bearer $auth1"}
