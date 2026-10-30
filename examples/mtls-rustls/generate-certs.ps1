param([string]$OutputDirectory = 'D:/BuildCache/salvo-mtls-example/certs')
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Path $OutputDirectory -ErrorAction Stop | Out-Null
Push-Location $OutputDirectory
try {
    function Invoke-OpenSsl {
        & openssl @args
        if ($LASTEXITCODE -ne 0) { throw 'OpenSSL failed' }
    }
    Invoke-OpenSsl req -x509 -newkey rsa:2048 -nodes -sha256 -days 30 -keyout ca.key -out ca.pem -subj '/CN=Salvo example CA' -addext 'basicConstraints=critical,CA:TRUE' -addext 'keyUsage=critical,keyCertSign,cRLSign'
    Invoke-OpenSsl req -new -newkey rsa:2048 -nodes -keyout server.key -out server.csr -subj '/CN=localhost' -addext 'subjectAltName=DNS:localhost,IP:127.0.0.1' -addext 'extendedKeyUsage=serverAuth' -addext 'basicConstraints=critical,CA:FALSE'
    Invoke-OpenSsl x509 -req -in server.csr -CA ca.pem -CAkey ca.key -set_serial 2 -days 30 -sha256 -copy_extensions copy -out server.pem
    foreach ($identity in @('reader', 'other')) {
        Invoke-OpenSsl req -new -newkey rsa:2048 -nodes -keyout "$identity.key" -out "$identity.csr" -subj "/CN=$identity" -addext "subjectAltName=URI:spiffe://salvo.local/client/$identity" -addext 'extendedKeyUsage=clientAuth' -addext 'basicConstraints=critical,CA:FALSE'
        $serial = if ($identity -eq 'reader') { 3 } else { 4 }
        Invoke-OpenSsl x509 -req -in "$identity.csr" -CA ca.pem -CAkey ca.key -set_serial $serial -days 30 -sha256 -copy_extensions copy -out "$identity.pem"
    }
} finally {
    Pop-Location
}
