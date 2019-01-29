{ lib, rustPlatform, pkgconfig, ncurses }:

rustPlatform.buildRustPackage rec {
  name = "${pname}-${version}";
  pname = "nix-update-fetch";
  version = "0.1.0";

  src = ./.;

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
