# Adapted from https://github.com/ramkulkarni1/django-apache2-docker/blob/master/Dockerfile
FROM ubuntu:18.04

RUN apt-get update
RUN apt-get install -y apt-utils vim curl apache2 apache2-utils
RUN apt-get -y install python3 libapache2-mod-wsgi-py3 python3-dev libmysqlclient-dev
RUN a2enmod rewrite
RUN ln /usr/bin/python3 /usr/bin/python
RUN apt-get -y install python3-pip
RUN ln /usr/bin/pip3 /usr/bin/pip
RUN mkdir /var/www/hosted
RUN chown -R www-data /var/www/hosted

RUN pip install --upgrade pip

ADD ./apache-config-inner.conf /etc/apache2/sites-available/000-default.conf
ADD . /var/www/phost

RUN pip install -r /var/www/phost/requirements.txt

CMD ["apache2ctl", "-D", "FOREGROUND"]
