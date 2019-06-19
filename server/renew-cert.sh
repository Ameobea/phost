#!/bin/bash

docker run -it --rm --name certbot-phost \
  -v "/etc/letsencrypt:/etc/letsencrypt" \
  -v "/var/lib/letsencrypt:/var/lib/letsencrypt" \
  -v "${PWD}:/root" \
  certbot/dns-digitalocean certonly --dns-digitalocean \
    --dns-digitalocean-credentials /root/digitalocean_creds.ini \
    --cert-name ameo.design \
    -d *.ameo.design \
    -d *.p.ameo.design \
    -d ameo.design

service apache2 restart
