# ECU Log Viewer

A log viewer for engine management logs. Try the [Github pages build](https://dankirkham.github.io/log-viewer/).

# Local development
All builds and tests are managed with a nix flake.

| Command             | Description                                |
| ---                 | ---                                        |
| `nix run`           | Runs the native application                |
| `nix run .#web-dev` | Run trunk to serve up the wasm web version |
| `nix develop`       | Dev shell for local development            |
| `nix flake check`   | Run local test suite                       |

# AI Usage
This application was built almost entirely by an AI agent (Codex/GPT-6-Luna). It is really only here so that I can host it on github pages.
