{ lib, callPackage, fetchFromGitHub, rustPlatform, pkgconfig, ncurses }:

with callPackage (fetchFromGitHub {
  owner = "siers";
  repo = "nix-gitignore";
  rev = "cc962a73113dbb32407d5099c4bf6f7ecf5612c9";
  sha256 = "08mgdnb54rhsz4024hx008dzg01c7kh3r45g068i7x91akjia2cq";
}) { };

rustPlatform.buildRustPackage rec {
  name = "${pname}-${version}";
  pname = "nix-update-fetch";
  version = "0.1.0";

  src = gitignoreSource [ ".git" ] ./.;

  RUSTC_BOOTSTRAP = 1;

  buildInputs = [ pkgconfig ncurses ];

  cargoSha256 = "0g2gmmhx2gcb02yqmzavx7fqyvdblgg16rhq10rw2slnrmsz84k6";

  meta = with lib; {
    description = "Prefetch any fetcher function call, e.g. a package source";
    homepage = https://github.com/msteen/nix-update-fetch;
    license = licenses.mit;
    maintainers = with maintainers; [ msteen ];
    platforms = platforms.all;
  };
}
