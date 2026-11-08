# tukzmon for Nix and NixOS. nix/package.nix is the package.
#
# Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
# SPDX-License-Identifier: LGPL-2.1-or-later
{
  description = "System monitor for the XFCE panel, with a log time axis";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs =
    { self, nixpkgs }:
    let
      # x86_64 only, as for the .deb and .rpm.
      system = "x86_64-linux";
      tukzmonFor = pkgs: pkgs.callPackage ./nix/package.nix { sourceDateEpoch = self.lastModified; };
    in
    {
      packages.${system} = rec {
        tukzmon = tukzmonFor nixpkgs.legacyPackages.${system};
        default = tukzmon;
      };

      overlays.default = final: _prev: { tukzmon = tukzmonFor final; };

      nixosModules.default =
        {
          config,
          lib,
          pkgs,
          ...
        }:
        let
          cfg = config.programs.tukzmon;
        in
        {
          options.programs.tukzmon = {
            enable = lib.mkEnableOption "tukzmon, the XFCE panel system monitor";
            package = lib.mkOption {
              type = lib.types.package;
              default = tukzmonFor pkgs;
              defaultText = lib.literalExpression "tukzmon.packages.\${system}.default";
              description = "The tukzmon package to use.";
            };
          };
          config = lib.mkIf cfg.enable {
            environment.systemPackages = [ cfg.package ];
            # The XFCE desktop module links these too. A panel run under
            # another desktop wouldn't find the plugin without them.
            environment.pathsToLink = [
              "/share/xfce4"
              "/lib/xfce4"
            ];
          };
        };
    };
}
