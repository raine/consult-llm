{
  description = "CLI for consulting stronger LLMs from your agent workflow";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      packageVersion = if builtins.isString cargoToml.package.version
        then cargoToml.package.version
        else cargoToml.workspace.package.version;
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.stdenv.mkDerivation {
            pname = cargoToml.package.name;
            version = packageVersion;

            src = ./.;

            nativeBuildInputs = with pkgs; [
              cargo
              rustc
              git
            ];

            buildPhase = ''
              runHook preBuild
              export CARGO_HOME="$TMPDIR/cargo-home"
              cargo build --release --locked
              runHook postBuild
            '';

            installPhase = ''
              runHook preInstall
              install -Dm755 target/release/consult-llm "$out/bin/consult-llm"
              runHook postInstall
            '';

            meta = with pkgs.lib; {
              description = "CLI for consulting stronger LLMs from your agent workflow";
              homepage = "https://github.com/raine/consult-llm";
              license = licenses.mit;
              mainProgram = "consult-llm";
            };
          };
        }
      );

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/consult-llm";
        };
      });

      devShells = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            buildInputs = with pkgs; [
              cargo
              rustc
              rust-analyzer
              rustfmt
              clippy
            ];

            RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
          };
        }
      );
    };
}
