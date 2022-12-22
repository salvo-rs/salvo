#!/bin/sh
# https://github.com/rustls/rustls/tree/main/test-ca
set -xe

rm -f *.pem *.p12
rm -rf rsa/
mkdir -p rsa/

openssl req -nodes \
          -x509 \
          -days 3650 \
          -newkey rsa:4096 \
          -keyout rsa/ca.key \
          -out rsa/ca.cert \
          -sha256 \
          -batch \
          -subj "/CN=ponytown RSA CA"

openssl req -nodes \
          -newkey rsa:3072 \
          -keyout rsa/inter.key \
          -out rsa/inter.req \
          -sha256 \
          -batch \
          -subj "/CN=ponytown RSA level 2 intermediate"

openssl req -nodes \
          -newkey rsa:2048 \
          -keyout rsa/end.key \
          -out rsa/end.req \
          -sha256 \
          -batch \
          -subj "/CN=testserver.com"

openssl rsa \
          -in rsa/end.key \
          -out rsa/end.rsa

openssl req -nodes \
          -newkey rsa:2048 \
          -keyout rsa/client.key \
          -out rsa/client.req \
          -sha256 \
          -batch \
          -subj "/CN=ponytown client"

openssl rsa \
          -in rsa/client.key \
          -out rsa/client.rsa

for kt in rsa ; do
  openssl x509 -req \
            -in $kt/inter.req \
            -out $kt/inter.cert \
            -CA $kt/ca.cert \
            -CAkey $kt/ca.key \
            -sha256 \
            -days 3650 \
            -set_serial 123 \
            -extensions v3_inter -extfile openssl.cnf

  openssl x509 -req \
            -in $kt/end.req \
            -out $kt/end.cert \
            -CA $kt/inter.cert \
            -CAkey $kt/inter.key \
            -sha256 \
            -days 2000 \
            -set_serial 456 \
            -extensions v3_end -extfile openssl.cnf

  openssl x509 -req \
            -in $kt/client.req \
            -out $kt/client.cert \
            -CA $kt/inter.cert \
            -CAkey $kt/inter.key \
            -sha256 \
            -days 2000 \
            -set_serial 789 \
            -extensions v3_client -extfile openssl.cnf

  cat $kt/inter.cert $kt/ca.cert > $kt/end.chain
  cat $kt/end.cert $kt/inter.cert $kt/ca.cert > $kt/end.fullchain

  cat $kt/inter.cert $kt/ca.cert > $kt/client.chain
  cat $kt/client.cert $kt/inter.cert $kt/ca.cert > $kt/client.fullchain

  openssl asn1parse -in $kt/ca.cert -out $kt/ca.der > /dev/null
done


cat rsa/inter.cert rsa/ca.cert > rsa/end.chain
cat rsa/end.cert rsa/inter.cert rsa/ca.cert > rsa/end.fullchain

cat rsa/inter.cert rsa/ca.cert > rsa/client.chain
cat rsa/client.cert rsa/inter.cert rsa/ca.cert > rsa/client.fullchain

openssl asn1parse -in rsa/ca.cert -out rsa/ca.der > /dev/null

cp rsa/end.key key.pem
cp rsa/end.cert cert.pem
cp rsa/end.fullchain chain.pem

openssl pkcs12 -export \
    -inkey rsa/inter.key \
    -in rsa/inter.cert \
    -certfile rsa/end.chain \
    -out identity.p12 \
    -passout pass:mypass

openssl pkcs12 -export -legacy \
    -inkey rsa/inter.key \
    -in rsa/inter.cert \
    -certfile rsa/end.chain \
    -out identity-legacy.p12 \
    -passout pass:mypass

rm -rf rsa