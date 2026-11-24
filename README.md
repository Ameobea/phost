# phost

`phost` as a utility that provides a range of functionality for deploying content and services on subdomains. It allows for static websites to be deployed from a directory on your local computer with a single command, Docker-based applications to be built + deployed remotely, and locally running services to be exposed on public subdomains via ngrok-style tunnels.

## Features

- Create deployments of static websites with a single command.
  - Automatically supports versioning where all previous versions can be accessed via a path extension
  - Supports SPA-style websites where a fallback file is served in the case of a 404
  - Supports randomly generated subdomains
- Expose locally running services on public subdomains: `phost tunnel <port>`
  - Serves `https://<subdomain>.p.<domain>/` from a single outbound websocket connection; works behind NAT with no ports opened
  - Supports WebSockets end-to-end and injects CORS headers by default (`--no-cors` to disable)
  - Picks a random git-short-hash-style subdomain unless `--subdomain` is passed; reconnects automatically

## Installation

The recommended way to run the server is via Docker. The whole service runs inside a single Docker container
