# phost

`phost` as a utility that provides a range of functionality for deploying content and services on subdomains. It allows for static websites to be deployed from a directory on your local computer with a single command as well as HTTP reverse proxy that allows accessing arbitrary HTTP web services from subdomains as well.

## Features

- Create deployments of static websites with a single command.
  - Automatically supports versioning where all previous versions can be accessed via a path extension
  - Supports SPA-style websites where a fallback file is served in the case of a 404
  - Supports randomly generated subdomains
- Create reverse HTTP proxies to arbitrary endpoints locally or on the internet
  - Has the option to transparently add in CORS headers to allow for the proxied resource to be accessed from browser applications

## Installation

The recommended way to run the server is via Docker. The whole service runs inside a single Docker container
