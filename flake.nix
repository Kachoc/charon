{
  description = "Charon";

  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "nixpkgs/nixos-unstable";
    # This makes it possible for downstream flakes to use a different nixpkgs
    # for our ocaml package, since we use `ocamlPackages` which points to a
    # different ocaml version depending on the nixpkgs version.
    nixpkgs-ocaml.follows = "nixpkgs";
    rust-overlay = {
      # We pin a specific commit because we require a relatively recent version
      # and flake dependents don't look at flake.lock.
      url = "github:oxalica/rust-overlay/275c824ed9e90e7fd4f96d187bde3670062e721f";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, flake-utils, nixpkgs, nixpkgs-ocaml, rust-overlay, crane }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        pkgs-ocaml = import nixpkgs-ocaml {
          inherit system;
        };

        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain;
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        charon = pkgs.callPackage ./nix/charon.nix { inherit craneLib rustToolchain; };
        charon-ml = pkgs-ocaml.callPackage ./nix/charon-ml.nix { inherit charon; };

        # Check rust files are correctly formatted.
        charon-check-fmt = charon.passthru.check-fmt;
        # Check that the crate builds with the "rustc" feature off.
        charon-check-no-rustc = charon.passthru.check-no-rustc;
        # Check ocaml files are correctly formatted.
        charon-ml-check-fmt = charon-ml.charon-ml-check-fmt;
        # Run ocaml tests
        charon-ml-tests = charon-ml.charon-ml-tests;

        # Runs charon on the whole rustc ui test suite.
        rustc-tests = pkgs.callPackage ./nix/rustc-tests.nix { inherit charon rustToolchain; };

        # Check that the generated ocaml files match what is committed to the repo.
        check-generated-ml = pkgs.runCommand "check-generated-ml" { } ''
          mkdir generated
          cp ${charon}/generated-ml/* generated
          chmod u+w generated/*
          cp ${./charon-ml/.ocamlformat} .ocamlformat
          ${pkgs-ocaml.ocamlPackages.ocamlformat}/bin/ocamlformat --inplace --enable-outside-detected-project generated/*.ml

          mkdir committed
          cp ${./charon-ml/src/generated}/*.ml committed

          if diff -rq committed generated; then
            echo "Ok: the regenerated ocaml files are the same as the checked out files"
          else
            echo "Error: the regenerated ocaml files differ from the checked out files"
            diff -ru committed generated
            exit 1
          fi
          touch $out
        '';

        # A utility that extracts the llbc of a crate using charon. This uses
        # `crane` to handle dependencies and toolchain management.
        extractCrateWithCharon = { name, src, charonFlags ? "", craneExtraArgs ? { } }:
          craneLib.buildPackage ({
            inherit name;
            src = pkgs.lib.cleanSourceWith {
              inherit src;
              filter = path: type: (craneLib.filterCargoSources path type);
            };
            cargoArtifacts = craneLib.buildDepsOnly { inherit src; };
            buildPhase = ''
              ${charon}/bin/charon ${charonFlags} --dest $out/llbc
            '';
            dontInstall = true;
          } // craneExtraArgs);
      in
      {
        packages = {
          inherit charon charon-ml rustToolchain;
          inherit (rustc-tests) toolchain_commit rustc-tests;
          default = charon;
        };
        devShells.default = pkgs.mkShell {
          # Tell charon that the right toolchain is in PATH. It is added to PATH by the `charon` in `inputsFrom`.
          CHARON_TOOLCHAIN_IS_IN_PATH = 1;
          # To run `cargo outdated` and `cargo udeps`
          LD_LIBRARY_PATH =
            pkgs.lib.makeLibraryPath [ pkgs.stdenv.cc.cc.lib pkgs.openssl pkgs.curl pkgs.zlib ];

          packages = [
            pkgs-ocaml.ocamlPackages.ocaml
            pkgs-ocaml.ocamlPackages.ocamlformat
            pkgs-ocaml.ocamlPackages.menhir
            pkgs-ocaml.ocamlPackages.odoc
            # ocamllsp's version must match the ocaml version used, hence we
            # can't an use externally-provided ocamllsp.
            pkgs-ocaml.ocamlPackages.ocaml-lsp
          ];

          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.rlwrap
          ];

          # To compile some rust crates that need system dependencies.
          buildInputs = [
            pkgs.openssl
            pkgs.glibc.out
            pkgs.glibc.static
          ];

          inputsFrom = [
            self.packages.${system}.charon
            self.packages.${system}.charon-ml
          ];
        };
        checks = {
          default = charon-ml-tests;
          inherit charon-ml-tests charon-check-fmt charon-check-no-rustc
            charon-ml-check-fmt check-generated-ml;
        };

        # Export this function so that users of charon can use it in nix. This
        # fits in none of the standard flake output categories hace why it is
        # exported directly like that.
        inherit extractCrateWithCharon;
      });
}
