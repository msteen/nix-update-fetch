{ nixpkgsPath ? <nixpkgs> }:

let
  mozOverlay = import (builtins.fetchTarball https://github.com/mozilla/nixpkgs-mozilla/archive/master.tar.gz);
  pkgs = import nixpkgsPath { overlays = [ mozOverlay ]; };

in pkgs.stdenv.mkDerivation {
  name = "moz-overlay-rust";
  buildInputs = [
    pkgs.latest.rustChannels.stable.rust
  ];
}
