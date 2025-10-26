{
  inputs = {
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    {
      nixpkgs,
      crane,
      flake-utils,
      ...
    }:

    let
      mkPackages =
        pkgs:
        with pkgs.lib;
        let
          craneLib = crane.mkLib pkgs;
        in
        {
          default = pkgs.callPackage (
            { rust, stdenv }:

            craneLib.buildPackage (
              {
                src = craneLib.cleanCargoSource ./.;
              }
              // (
                with rust.envVars;
                with stdenv;
                {
                  "CARGO_BUILD_TARGET" = rustHostPlatform;

                  "CC_${stdenv.hostPlatform.rust.cargoEnvVarTarget}" = ccForHost;
                  "CXX_${stdenv.hostPlatform.rust.cargoEnvVarTarget}" = cxxForHost;
                  "CARGO_TARGET_${stdenv.hostPlatform.rust.cargoEnvVarTarget}_LINKER" = ccForHost;
                }
                // optionalAttrs (stdenv.hostPlatform != stdenv.buildPlatform) {
                  "CC_${stdenv.buildPlatform.rust.cargoEnvVarTarget}" = ccForBuild;
                  "CXX_${stdenv.buildPlatform.rust.cargoEnvVarTarget}" = cxxForBuild;
                  "CARGO_TARGET_${stdenv.buildPlatform.rust.cargoEnvVarTarget}_LINKER" = ccForBuild;
                  "HOST_CC" = ccForBuild;
                  "HOST_CXX" = cxxForBuild;
                }
              )
            )
          ) { };
        };

    in
    flake-utils.lib.eachDefaultSystem (system: {
      packages = mkPackages (import nixpkgs { inherit system; });
    })
    // {
      nixosModules.default =
        {
          config,
          pkgs,
          lib,
          ...
        }:
        with lib;
        let
          svcConfig = config.services.sni-router;
          package = (mkPackages pkgs).default;
          toml = pkgs.formats.toml { };
        in
        {
          options = {
            services.sni-router = {
              enable = mkOption {
                type = types.bool;
                default = false;
              };
              environmentFile = mkOption {
                type = types.nullOr types.path;
                default = null;
              };
              config = mkOption {
                type = toml.type;
                default = { };
              };
            };
          };

          config = mkIf svcConfig.enable {
            systemd.services.sni-router = {
              wants = [ "network-online.target" ];
              after = [ "network-online.target" ];
              wantedBy = [ "multi-user.target" ];

              environment = {
                SNI_ROUTER_CONFIG = toml.generate "sni-router.toml" svcConfig.config;
              };

              serviceConfig = mkMerge [
                {
                  EnvironmentFile = svcConfig.environmentFile;
                  ExecStart = "${package}/bin/sni-router";
                  Restart = "on-failure";
                  RestartSec = 3;
                }
              ];
            };
          };
        };
    };
}
