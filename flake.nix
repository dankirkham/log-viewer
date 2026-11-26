{
  description = "Local ECU log viewer";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    crane.url = "github:ipetkov/crane";
    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };
  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      crane,
      advisory-db,
    }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ (import rust-overlay) ];
      };
      isSampleAsset =
        path: _type:
        baseNameOf path == "samples" || pkgs.lib.hasPrefix "${toString ./.}/samples/" (toString path);
      toolchain = pkgs.rust-bin.stable."1.88.0".minimal.override {
        extensions = [
          "rust-src"
          "rustfmt"
          "clippy"
        ];
        targets = [ "wasm32-unknown-unknown" ];
      };
      craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;
      src = pkgs.lib.cleanSourceWith {
        src = ./.;
        filter =
          path: type:
          craneLib.filterCargoSources path type
          || baseNameOf path == "sanitized.mlg"
          || isSampleAsset path type;
      };
      libs = with pkgs; [
        libxkbcommon
        wayland
        libGL
        libx11
        libxcursor
        libxi
        libxrandr
      ];
      common = {
        inherit src;
        strictDeps = true;
        nativeBuildInputs = [ pkgs.pkg-config ];
        buildInputs = libs;
      };
      artifacts = craneLib.buildDepsOnly common;
      native = craneLib.buildPackage (
        common
        // {
          cargoArtifacts = artifacts;
          nativeBuildInputs = common.nativeBuildInputs ++ [ pkgs.makeWrapper ];
          postFixup = "wrapProgram $out/bin/log-viewer --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath libs}";
        }
      );
      web = craneLib.mkCargoDerivation (
        common
        // {
          cargoArtifacts = null;
          doInstallCargoArtifacts = false;
          pname = "log-viewer-web";
          version = "0.1.0";
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter =
              path: type:
              craneLib.filterCargoSources path type || baseNameOf path == "index.html" || isSampleAsset path type;
          };
          nativeBuildInputs = common.nativeBuildInputs ++ [
            pkgs.trunk
            pkgs.wasm-bindgen-cli
            pkgs.binaryen
          ];
          buildPhaseCargoCommand = "trunk build --skip-version-check --release --public-url ./";
          installPhaseCommand = "mkdir -p $out; cp -r dist/. $out/";
        }
      );
      smokeWeb = craneLib.mkCargoDerivation (
        common
        // {
          cargoArtifacts = null;
          doInstallCargoArtifacts = false;
          pname = "log-viewer-browser-smoke";
          version = "0.1.0";
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter =
              path: type:
              craneLib.filterCargoSources path type
              || builtins.elem (baseNameOf path) [ "index-smoke.html" ]
              || isSampleAsset path type;
          };
          nativeBuildInputs = common.nativeBuildInputs ++ [
            pkgs.trunk
            pkgs.wasm-bindgen-cli
            pkgs.binaryen
          ];
          buildPhaseCargoCommand = "trunk build index-smoke.html --skip-version-check --release --public-url ./ --dist dist-smoke";
          preBuild = ''
            export CARGO_ENCODED_RUSTFLAGS="--cfg=browser_smoke"
          '';
          installPhaseCommand = "mkdir -p $out; cp -r dist-smoke/. $out/";
        }
      );
    in
    {
      packages.${system} = {
        default = native;
        inherit web;
      };
      apps.${system}.web-dev = {
        type = "app";
        program = "${pkgs.writeShellScript "web-dev" ''
          export PATH=${
            pkgs.lib.makeBinPath [
              toolchain
              pkgs.wasm-bindgen-cli
              pkgs.pkg-config
            ]
          }:$PATH
          exec ${pkgs.trunk}/bin/trunk serve --skip-version-check --public-url ./ "$@"
        ''}";
      };
      devShells.${system}.default = pkgs.mkShell {
        packages = [
          toolchain
          pkgs.pkg-config
          pkgs.trunk
          pkgs.wasm-bindgen-cli
          pkgs.binaryen
          pkgs.nixfmt
          pkgs.cargo-audit
          pkgs.chromium
          pkgs.chromedriver
          (pkgs.python3.withPackages (pythonPackages: [ pythonPackages.selenium ]))
        ];
        buildInputs = libs;
        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath libs;
      };
      checks.${system} = {
        inherit native web;
        audit = pkgs.runCommand "cargo-audit" { nativeBuildInputs = [ pkgs.cargo-audit ]; } ''
          export HOME=$TMPDIR
          cargo-audit audit --file ${./Cargo.lock} --db ${advisory-db} --no-fetch
          touch $out
        '';
        browser-smoke =
          pkgs.runCommand "browser-smoke"
            {
              nativeBuildInputs = [
                pkgs.chromium
                pkgs.chromedriver
                (pkgs.python3.withPackages (pythonPackages: [ pythonPackages.selenium ]))
              ];
            }
            ''
              export HOME="$TMPDIR/home"
              mkdir -p "$HOME" "$TMPDIR/web"
              cp -r ${smokeWeb}/. "$TMPDIR/web/"
              cp ${./tests/fixtures/sanitized.mlg} "$TMPDIR/web/sanitized.mlg"
              python3 -m http.server 8765 --bind 127.0.0.1 --directory "$TMPDIR/web" > "$TMPDIR/server.log" 2>&1 &
              server_pid=$!
              trap 'kill "$server_pid" 2>/dev/null || true' EXIT
              python3 ${./tests/browser_smoke.py} \
                --url http://127.0.0.1:8765/ \
                --browser ${pkgs.chromium}/bin/chromium \
                --driver ${pkgs.chromedriver}/bin/chromedriver \
                --fixture-url /sanitized.mlg \
                --fixture-name sanitized.mlg
              touch $out
            '';
        fmt = craneLib.cargoFmt { inherit src; };
        clippy = craneLib.cargoClippy (
          common
          // {
            cargoArtifacts = artifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          }
        );
        clippy-web = craneLib.cargoClippy (
          common
          // {
            cargoArtifacts = null;
            cargoExtraArgs = "--target wasm32-unknown-unknown";
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          }
        );
        tests = craneLib.cargoTest (common // { cargoArtifacts = artifacts; });
        nixfmt = pkgs.runCommand "nixfmt-check" {
          nativeBuildInputs = [ pkgs.nixfmt ];
        } "nixfmt --check ${./flake.nix}; touch $out";
      };
    };
}
