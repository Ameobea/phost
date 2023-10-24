# This has to match with the inner container due to shared library mismatches etc.
FROM ubuntu:18.04 AS builder

RUN apt-get update && apt-get install -y curl libmysqlclient-dev build-essential libssl-dev pkg-config
RUN update-ca-certificates

# Install rust
RUN curl https://sh.rustup.rs/ -sSf | \
  sh -s -- -y --default-toolchain nightly-2023-10-23

ENV PATH="/root/.cargo/bin:${PATH}"

ADD ./proxy /proxy
WORKDIR /proxy

RUN cargo build --release

# Adapted from https://github.com/ramkulkarni1/django-apache2-docker/blob/master/Dockerfile
# This has to match with the inner container due to shared library mismatches etc.
FROM ubuntu:18.04

COPY --from=builder \
  /proxy/target/release/phost-proxy \
  /usr/local/bin/phost-proxy

RUN apt-get update && apt-get install -y vim curl apache2 apache2-utils
RUN apt-get -y install python3 libapache2-mod-wsgi-py3 python3-dev libmysqlclient-dev
RUN a2enmod rewrite
RUN ln /usr/bin/python3 /usr/bin/python
RUN apt-get -y install python3-pip
RUN ln /usr/bin/pip3 /usr/bin/pip
RUN mkdir /var/www/hosted
RUN chown -R www-data /var/www/hosted

RUN pip install --upgrade pip

ADD ./server/requirements.txt /var/www/phost/requirements.txt
RUN pip install -r /var/www/phost/requirements.txt

ADD ./server/apache-config-inner.conf /etc/apache2/sites-available/000-default.conf

ADD ./server /var/www/phost

CMD ["apache2ctl", "-D", "FOREGROUND"]
